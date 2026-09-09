import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ShortcutRecorder, { formatShortcut } from "./Shortcut";

type Status = {
  microphone: boolean;
  accessibility: boolean;
  modelInstalled: boolean;
  cleanupModelInstalled: boolean;
};

type Settings = {
  cleanupEnabled: boolean;
  dictionary: string[];
  engine: string;
  whisperModel: string;
  language: string | null;
  hotkey: string;
  hotkeyMode: string;
  soundsEnabled: boolean;
  smartFormatting: boolean;
};

function Toggle({
  on,
  onClick,
}: {
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      role="switch"
      aria-checked={on}
      className={`relative h-6 w-10 shrink-0 rounded-full transition-colors ${
        on ? "bg-blue-500" : "bg-neutral-300 dark:bg-neutral-600"
      }`}
    >
      <span
        className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all ${
          on ? "left-[18px]" : "left-0.5"
        }`}
      />
    </button>
  );
}

type Download = {
  model: string;
  file: string;
  downloaded: number;
  total: number | null;
  done: boolean;
};

function Row({
  ok,
  label,
  detail,
  action,
  onAction,
}: {
  ok: boolean;
  label: string;
  detail: string;
  action?: string;
  onAction?: () => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
      <div className="flex items-center gap-3">
        <span
          className={`flex h-6 w-6 items-center justify-center rounded-full text-sm ${
            ok
              ? "bg-green-500/15 text-green-600 dark:text-green-400"
              : "bg-amber-500/15 text-amber-600 dark:text-amber-400"
          }`}
        >
          {ok ? "✓" : "!"}
        </span>
        <div>
          <p className="text-sm font-medium">{label}</p>
          <p className="text-xs text-neutral-500 dark:text-neutral-400">{detail}</p>
        </div>
      </div>
      {!ok && action && (
        <button
          onClick={onAction}
          className="rounded-md bg-blue-500 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-600"
        >
          {action}
        </button>
      )}
    </div>
  );
}

export default function General() {
  const [status, setStatus] = useState<Status | null>(null);
  const [download, setDownload] = useState<Download | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);

  const refresh = () => {
    invoke<Status>("get_status").then(setStatus).catch(console.error);
  };

  useEffect(() => {
    refresh();
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
    const interval = setInterval(refresh, 3000);
    const unlisten = listen<Download>("model-download", (e) => {
      setDownload(e.payload.done ? null : e.payload);
      if (e.payload.done) refresh();
    });
    return () => {
      clearInterval(interval);
      unlisten.then((f) => f());
    };
  }, []);

  const [autostart, setAutostart] = useState(false);
  useEffect(() => {
    invoke<boolean>("get_autostart").then(setAutostart).catch(console.error);
  }, []);

  const request = (name: string) =>
    invoke("request_permission", { name }).then(refresh);

  const save = (next: Settings) => {
    setSettings(next);
    invoke("set_settings", { newSettings: next }).catch(console.error);
  };

  const toggleCleanup = () => {
    if (!settings) return;
    const next = { ...settings, cleanupEnabled: !settings.cleanupEnabled };
    save(next);
    if (next.cleanupEnabled && status && !status.cleanupModelInstalled) {
      invoke("download_cleanup_model").then(refresh).catch(console.error);
    }
  };

  const hotkeyLabel = formatShortcut(settings?.hotkey ?? "right-cmd");
  const toggleMode = settings?.hotkeyMode === "toggle";

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3">
      <h2 className="text-base font-semibold">Setup</h2>
      {status && (
        <>
          <Row
            ok={status.microphone}
            label="Microphone"
            detail={
              status.microphone
                ? "Needed to hear you dictate"
                : "Needed to hear you dictate. If no prompt appears, System Settings opens — enable Vespry there, then relaunch."
            }
            action="Grant"
            onAction={() => request("microphone")}
          />
          <Row
            ok={status.accessibility}
            label="Accessibility"
            detail={
              status.accessibility
                ? "Needed to type into other apps"
                : "Needed to type into other apps. Already granted? An app update invalidates the old grant — toggle Vespry off and on in System Settings → Accessibility, then relaunch."
            }
            action="Grant"
            onAction={() => request("accessibility")}
          />
          <Row
            ok={status.modelInstalled}
            label="Speech model"
            detail={
              download
                ? `Downloading ${download.file}… ${
                    download.total
                      ? Math.round((download.downloaded / download.total) * 100)
                      : "?"
                  }%`
                : status.modelInstalled
                  ? "Parakeet v3 ready — runs fully on-device"
                  : "Downloads automatically (~640 MB)"
            }
          />
          {download && download.total && (
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-black/10 dark:bg-white/10">
              <div
                className="h-full rounded-full bg-blue-500 transition-all"
                style={{
                  width: `${(download.downloaded / download.total) * 100}%`,
                }}
              />
            </div>
          )}
        </>
      )}

      <h2 className="mt-4 text-base font-semibold">AI cleanup</h2>
      <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <div>
          <p className="text-sm font-medium">Clean up transcripts</p>
          <p className="text-xs text-neutral-500 dark:text-neutral-400">
            {status?.cleanupModelInstalled
              ? "Qwen3 removes filler words and fixes punctuation, fully on-device"
              : "Downloads Qwen3 4B (~2.4 GB) on first enable"}
          </p>
        </div>
        <Toggle on={settings?.cleanupEnabled ?? false} onClick={toggleCleanup} />
      </div>
      {settings?.cleanupEnabled && (
        <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
          <div>
            <p className="text-sm font-medium">Format lists automatically</p>
            <p className="text-xs text-neutral-500 dark:text-neutral-400">
              Counting items off ("first… second… third") becomes a bulleted or
              numbered list
            </p>
          </div>
          <Toggle
            on={settings?.smartFormatting ?? true}
            onClick={() =>
              settings &&
              save({ ...settings, smartFormatting: !settings.smartFormatting })
            }
          />
        </div>
      )}

      <h2 className="mt-4 text-base font-semibold">Behavior</h2>
      <div className="flex items-start justify-between gap-4 rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <div>
          <p className="text-sm font-medium">Shortcut</p>
          <p className="text-xs text-neutral-500 dark:text-neutral-400">
            Click, then hold the keys you want. A modifier on its own — ⌘, ⌥,
            ⌃ — works best: Vespry watches keys rather than swallowing them, so
            a shortcut with a letter still types that letter.
          </p>
        </div>
        <ShortcutRecorder
          value={settings?.hotkey ?? "right-cmd"}
          onChange={(hotkey) => settings && save({ ...settings, hotkey })}
        />
      </div>
      <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <div>
          <p className="text-sm font-medium">Activation</p>
          <p className="text-xs text-neutral-500 dark:text-neutral-400">
            {toggleMode
              ? "Press once to start, again to stop"
              : "Dictate for as long as you hold the shortcut"}
          </p>
        </div>
        <select
          value={settings?.hotkeyMode ?? "hold"}
          onChange={(e) =>
            settings && save({ ...settings, hotkeyMode: e.target.value })
          }
          className="max-w-56 rounded-md border border-black/10 bg-white px-2 py-1 text-sm dark:border-white/10 dark:bg-neutral-800"
        >
          <option value="hold">Hold to talk</option>
          <option value="toggle">Press to start and stop</option>
        </select>
      </div>
      <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <p className="text-sm font-medium">Start & stop sounds</p>
        <Toggle
          on={settings?.soundsEnabled ?? true}
          onClick={() =>
            settings &&
            save({ ...settings, soundsEnabled: !settings.soundsEnabled })
          }
        />
      </div>
      <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <p className="text-sm font-medium">Launch at login</p>
        <Toggle
          on={autostart}
          onClick={() => {
            const next = !autostart;
            setAutostart(next);
            invoke("set_autostart", { enabled: next }).catch(console.error);
          }}
        />
      </div>

      <h2 className="mt-4 text-base font-semibold">How to dictate</h2>
      <div className="rounded-lg border border-black/10 bg-white/60 px-4 py-3 text-sm leading-6 dark:border-white/10 dark:bg-white/5">
        {toggleMode ? (
          <p>
            <b>Press {hotkeyLabel}</b> and speak, press it again to insert the
            text wherever your cursor is.
          </p>
        ) : (
          <p>
            <b>Hold {hotkeyLabel}</b> and speak, release to insert the text
            wherever your cursor is.
          </p>
        )}
        <p className="text-neutral-500 dark:text-neutral-400">
          Start / Stop Dictation in the menu bar always runs hands-free.
        </p>
      </div>

      <h2 className="mt-4 text-base font-semibold">Try it</h2>
      <textarea
        className="min-h-24 w-full resize-none rounded-lg border border-black/10 bg-white p-3 text-sm outline-none focus:border-blue-400 dark:border-white/10 dark:bg-white/10"
        placeholder={`Click here, then ${
          toggleMode ? "press" : "hold"
        } ${hotkeyLabel} and speak…`}
      />
    </div>
  );
}
