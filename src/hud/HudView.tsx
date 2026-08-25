import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type DictationState =
  | { state: "idle" }
  | { state: "listening"; hands_free: boolean }
  | { state: "transcribing" }
  | { state: "injecting" }
  | { state: "error"; message: string };

const BAR_COUNT = 22;

/** The floating dictation pill: live waveform while listening, bouncing
 * dots while transcribing, brief error text on failure. */
export default function HudView() {
  const [mode, setMode] = useState<DictationState>({ state: "idle" });
  const levelRef = useRef(0);
  const barsRef = useRef<(HTMLDivElement | null)[]>([]);
  const phases = useRef(
    Array.from({ length: BAR_COUNT }, () => Math.random() * Math.PI * 2),
  );

  useEffect(() => {
    const unState = listen<DictationState>("dictation-state", (e) => {
      setMode(e.payload);
      if (e.payload.state === "listening") levelRef.current = 0;
    });
    const unLevel = listen<number>("audio-level", (e) => {
      // Smooth toward the incoming RMS; boost quiet speech a bit.
      const boosted = Math.min(1, e.payload * 9);
      levelRef.current = levelRef.current * 0.6 + boosted * 0.4;
    });
    return () => {
      unState.then((f) => f());
      unLevel.then((f) => f());
    };
  }, []);

  const listening = mode.state === "listening";

  useEffect(() => {
    if (!listening) return;
    let raf: number;
    const animate = () => {
      const t = performance.now() / 1000;
      const level = levelRef.current;
      barsRef.current.forEach((bar, i) => {
        if (!bar) return;
        // Center-weighted bars with per-bar wobble, driven by mic level.
        const center = 1 - Math.abs(i - (BAR_COUNT - 1) / 2) / (BAR_COUNT / 2);
        const wobble = 0.6 + 0.4 * Math.sin(t * 9 + phases.current[i]);
        const h = 3 + level * 26 * (0.35 + 0.65 * center) * wobble;
        bar.style.height = `${h.toFixed(1)}px`;
      });
      raf = requestAnimationFrame(animate);
    };
    raf = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(raf);
  }, [listening]);

  return (
    <div className="flex h-screen items-end justify-center bg-transparent pb-1">
      <div
        className={`flex h-11 items-center justify-center gap-[3px] rounded-full px-5 shadow-lg backdrop-blur-md transition-all duration-200 ${
          mode.state === "error" ? "bg-red-950/90" : "bg-neutral-950/90"
        }`}
      >
        {listening &&
          Array.from({ length: BAR_COUNT }, (_, i) => (
            <div
              key={i}
              ref={(el) => {
                barsRef.current[i] = el;
              }}
              className={`w-[3px] rounded-full ${
                mode.state === "listening" && mode.hands_free
                  ? "bg-sky-300"
                  : "bg-white"
              }`}
              style={{ height: 3, transition: "height 60ms linear" }}
            />
          ))}
        {(mode.state === "transcribing" || mode.state === "injecting") && (
          <div className="flex items-center gap-1.5">
            {[0, 1, 2].map((i) => (
              <div
                key={i}
                className="h-2 w-2 animate-bounce rounded-full bg-white/90"
                style={{ animationDelay: `${i * 120}ms` }}
              />
            ))}
          </div>
        )}
        {mode.state === "error" && (
          <span className="max-w-64 truncate text-xs text-red-200">
            {mode.message}
          </span>
        )}
        {mode.state === "idle" && (
          <div className="h-1.5 w-10 rounded-full bg-white/50" />
        )}
      </div>
    </div>
  );
}
