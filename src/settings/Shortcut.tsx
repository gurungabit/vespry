import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Shortcuts are stored as `+`-joined specs the Rust side parses: modifier
 * tokens (`right-cmd`, `ctrl`, `fn`, …) plus an optional physical key named
 * by its DOM `code` (`KeyD`, `F5`, `Space`).
 */
type Modifier = { family: number; symbol: string; long: string };

const MODIFIERS: Record<string, Modifier> = {
  fn: { family: 0, symbol: "🌐", long: "Fn 🌐" },
  ctrl: { family: 1, symbol: "⌃", long: "Control ⌃" },
  "left-ctrl": { family: 1, symbol: "L⌃", long: "Left ⌃" },
  "right-ctrl": { family: 1, symbol: "R⌃", long: "Right ⌃" },
  alt: { family: 2, symbol: "⌥", long: "Option ⌥" },
  "left-alt": { family: 2, symbol: "L⌥", long: "Left ⌥" },
  "right-alt": { family: 2, symbol: "R⌥", long: "Right ⌥" },
  shift: { family: 3, symbol: "⇧", long: "Shift ⇧" },
  "left-shift": { family: 3, symbol: "L⇧", long: "Left ⇧" },
  "right-shift": { family: 3, symbol: "R⇧", long: "Right ⇧" },
  cmd: { family: 4, symbol: "⌘", long: "Command ⌘" },
  "left-cmd": { family: 4, symbol: "L⌘", long: "Left ⌘" },
  "right-cmd": { family: 4, symbol: "R⌘", long: "Right ⌘" },
};

const KEY_LABELS: Record<string, string> = {
  Space: "Space",
  Enter: "↩",
  Tab: "⇥",
  Escape: "Esc",
  Backspace: "⌫",
  Delete: "⌦",
  Home: "↖",
  End: "↘",
  PageUp: "⇞",
  PageDown: "⇟",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

function keyLabel(code: string): string {
  if (/^f\d+$/i.test(code)) return code.toUpperCase();
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return KEY_LABELS[code] ?? code;
}

/** Render a spec for humans: "Right ⌘", "⌃ ⇧ D". */
export function formatShortcut(spec: string): string {
  const parts = spec.split("+").map((p) => p.trim()).filter(Boolean);
  const mods = parts
    .filter((p) => p in MODIFIERS)
    .sort((a, b) => MODIFIERS[a].family - MODIFIERS[b].family);
  const key = parts.find((p) => !(p in MODIFIERS));
  // A lone modifier reads better spelled out — that's the common case.
  if (mods.length === 1 && !key) return MODIFIERS[mods[0]].long;
  const labels = mods.map((m) => MODIFIERS[m].symbol);
  if (key) labels.push(keyLabel(key));
  return labels.join(" ") || "None";
}

/** DOM `code` of a modifier → the spec token for that exact key. */
const CODE_TO_MOD: Record<string, string> = {
  MetaLeft: "left-cmd",
  MetaRight: "right-cmd",
  ControlLeft: "left-ctrl",
  ControlRight: "right-ctrl",
  AltLeft: "left-alt",
  AltRight: "right-alt",
  ShiftLeft: "left-shift",
  ShiftRight: "right-shift",
};

/** Modifier-only shortcuts that can't be recorded, or are just worth one click. */
const PRESETS = ["right-cmd", "right-alt", "left-ctrl", "fn"];

function toSpec(tokens: string[]): string {
  const mods = tokens
    .filter((t) => t in MODIFIERS)
    .sort((a, b) => MODIFIERS[a].family - MODIFIERS[b].family);
  const key = tokens.find((t) => !(t in MODIFIERS));
  return [...mods, ...(key ? [key] : [])].join("+");
}

export default function ShortcutRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (spec: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const heldRef = useRef<string[]>([]);
  // The most modifiers held at once: what a modifier-only shortcut commits
  // as, once everything is released.
  const pendingRef = useRef<string[]>([]);

  // The global listener polls key state, so recording here would otherwise
  // start a dictation at the same time.
  const setCapture = (active: boolean) =>
    invoke("set_shortcut_capture", { active }).catch(console.error);

  const stop = () => {
    setRecording(false);
    setHeld([]);
    heldRef.current = [];
    pendingRef.current = [];
    setCapture(false);
  };

  const start = () => {
    setError(null);
    setHeld([]);
    heldRef.current = [];
    pendingRef.current = [];
    setRecording(true);
    setCapture(true);
  };

  const commit = (tokens: string[]) => {
    if (tokens.length === 0) return;
    onChange(toSpec(tokens));
    stop();
  };

  useEffect(() => {
    if (!recording) return;

    const setHeldKeys = (next: string[]) => {
      heldRef.current = next;
      setHeld(next);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      const mod = CODE_TO_MOD[e.code];
      if (mod) {
        if (heldRef.current.includes(mod)) return;
        const next = [...heldRef.current, mod];
        setHeldKeys(next);
        if (next.length >= pendingRef.current.length) pendingRef.current = next;
        return;
      }
      if (e.code === "Escape" && heldRef.current.length === 0) {
        stop();
        return;
      }
      // A bare letter would fire on every word the user types anywhere.
      if (heldRef.current.length === 0 && !/^F\d+$/.test(e.code)) {
        setError("Hold a modifier too — a bare key would fire as you type.");
        return;
      }
      commit([...heldRef.current, e.code]);
    };

    const onKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const mod = CODE_TO_MOD[e.code];
      if (!mod) return;
      const next = heldRef.current.filter((m) => m !== mod);
      setHeldKeys(next);
      // Everything let go without a key being pressed → modifier-only.
      if (next.length === 0) commit(pendingRef.current);
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", stop);
    };
  }, [recording]);

  // Never leave the global shortcut muted if this pane goes away mid-record.
  useEffect(() => () => void setCapture(false), []);

  const preview = held.length > 0 ? formatShortcut(toSpec(held)) : "Press keys…";

  return (
    <div className="flex flex-col items-end gap-2">
      <button
        onClick={recording ? stop : start}
        className={`min-w-32 rounded-md border px-3 py-1.5 text-sm font-medium tabular-nums ${
          recording
            ? "border-blue-400 bg-blue-500/10 text-blue-600 dark:text-blue-400"
            : "border-black/10 bg-white hover:bg-black/5 dark:border-white/10 dark:bg-neutral-800 dark:hover:bg-white/10"
        }`}
      >
        {recording ? preview : formatShortcut(value)}
      </button>
      <div className="flex flex-wrap justify-end gap-1">
        {PRESETS.map((spec) => (
          <button
            key={spec}
            onClick={() => {
              stop();
              setError(null);
              onChange(spec);
            }}
            className={`rounded-md border px-2 py-0.5 text-xs ${
              value === spec
                ? "border-blue-400 text-blue-600 dark:text-blue-400"
                : "border-black/10 text-neutral-500 hover:bg-black/5 dark:border-white/10 dark:text-neutral-400 dark:hover:bg-white/10"
            }`}
          >
            {formatShortcut(spec)}
          </button>
        ))}
      </div>
      {recording && (
        <p className="max-w-64 text-right text-xs text-neutral-500 dark:text-neutral-400">
          Hold the keys you want, then let go. Esc cancels.
        </p>
      )}
      {error && (
        <p className="max-w-64 text-right text-xs text-amber-600 dark:text-amber-400">
          {error}
        </p>
      )}
    </div>
  );
}
