import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type ModelInfo = {
  id: string;
  label: string;
  sizeMb: number;
  installed: boolean;
  kind: "asr" | "cleanup";
};

type Settings = {
  cleanupEnabled: boolean;
  dictionary: string[];
  engine: string;
  whisperModel: string;
  language: string | null;
};

type Download = {
  model: string;
  file: string;
  downloaded: number;
  total: number | null;
  done: boolean;
};

const LANGUAGES: [string, string][] = [
  ["", "Auto-detect"],
  ["en", "English"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["pt", "Portuguese"],
  ["it", "Italian"],
  ["nl", "Dutch"],
  ["ru", "Russian"],
  ["zh", "Chinese"],
  ["ja", "Japanese"],
  ["ko", "Korean"],
  ["hi", "Hindi"],
  ["ar", "Arabic"],
  ["ne", "Nepali"],
];

export default function Models() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [downloads, setDownloads] = useState<Record<string, Download>>({});
  const [pending, setPending] = useState<Record<string, boolean>>({});

  const refresh = () => {
    invoke<ModelInfo[]>("list_models").then(setModels).catch(console.error);
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
  };

  useEffect(() => {
    refresh();
    const unlisten = listen<Download>("model-download", (e) => {
      setDownloads((d) => {
        const next = { ...d };
        if (e.payload.done) delete next[e.payload.model];
        else next[e.payload.model] = e.payload;
        return next;
      });
      if (e.payload.done) refresh();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const save = (next: Settings) => {
    setSettings(next);
    invoke("set_settings", { newSettings: next }).catch(console.error);
  };

  const download = (id: string) => {
    setPending((p) => ({ ...p, [id]: true }));
    invoke("download_model", { id })
      .catch(console.error)
      .finally(() => {
        setPending((p) => ({ ...p, [id]: false }));
        refresh();
      });
  };

  const selectAsr = (m: ModelInfo) => {
    if (!settings) return;
    const next =
      m.id === "parakeet"
        ? { ...settings, engine: "parakeet" }
        : { ...settings, engine: "whisper", whisperModel: m.id };
    save(next);
    if (!m.installed) download(m.id);
  };

  const isSelected = (m: ModelInfo) =>
    settings != null &&
    (m.id === "parakeet"
      ? settings.engine === "parakeet"
      : settings.engine === "whisper" && settings.whisperModel === m.id);

  const progressFor = (m: ModelInfo) =>
    downloads[m.id === "qwen" ? "qwen3-1.7b-q4km" : m.id === "parakeet" ? "parakeet-tdt-0.6b-v3-int8" : m.id];

  const renderModel = (m: ModelInfo) => {
    const dl = progressFor(m);
    return (
      <div
        key={m.id}
        onClick={() => m.kind === "asr" && selectAsr(m)}
        className={`flex cursor-pointer items-center justify-between rounded-lg border px-4 py-3 transition-colors ${
          isSelected(m)
            ? "border-blue-500 bg-blue-500/10"
            : "border-black/10 bg-white/60 hover:border-black/25 dark:border-white/10 dark:bg-white/5 dark:hover:border-white/25"
        }`}
      >
        <div className="flex items-center gap-3">
          {m.kind === "asr" && (
            <span
              className={`h-4 w-4 rounded-full border-2 ${
                isSelected(m)
                  ? "border-blue-500 bg-blue-500"
                  : "border-neutral-400"
              }`}
            />
          )}
          <div>
            <p className="text-sm font-medium">{m.label}</p>
            <p className="text-xs text-neutral-500 dark:text-neutral-400">
              {dl && dl.total
                ? `Downloading… ${Math.round((dl.downloaded / dl.total) * 100)}%`
                : m.installed
                  ? `${m.sizeMb} MB · installed`
                  : `${m.sizeMb} MB download`}
            </p>
          </div>
        </div>
        {!m.installed && !dl && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              download(m.id);
            }}
            disabled={pending[m.id]}
            className="rounded-md bg-blue-500 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {pending[m.id] ? "…" : "Download"}
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3">
      <h2 className="text-base font-semibold">Speech recognition</h2>
      <p className="text-xs text-neutral-500 dark:text-neutral-400">
        Parakeet is the fastest. Pick a Whisper model for languages Parakeet
        doesn't cover (it supports ~100).
      </p>
      {models.filter((m) => m.kind === "asr").map(renderModel)}

      {settings?.engine === "whisper" && (
        <div className="flex items-center justify-between rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5">
          <p className="text-sm font-medium">Spoken language</p>
          <select
            value={settings.language ?? ""}
            onChange={(e) =>
              save({ ...settings, language: e.target.value || null })
            }
            className="rounded-md border border-black/10 bg-white px-2 py-1 text-sm dark:border-white/10 dark:bg-neutral-800"
          >
            {LANGUAGES.map(([code, label]) => (
              <option key={code} value={code}>
                {label}
              </option>
            ))}
          </select>
        </div>
      )}

      <h2 className="mt-4 text-base font-semibold">Cleanup</h2>
      {models.filter((m) => m.kind === "cleanup").map(renderModel)}
    </div>
  );
}
