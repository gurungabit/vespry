import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Settings = {
  cleanupEnabled: boolean;
  dictionary: string[];
  engine: string;
  whisperModel: string;
  language: string | null;
  hotkey: string;
  soundsEnabled: boolean;
};

export default function Dictionary() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings).catch(console.error);
  }, []);

  const save = (next: Settings) => {
    setSettings(next);
    invoke("set_settings", { newSettings: next }).catch(console.error);
  };

  const add = () => {
    const term = draft.trim();
    if (!settings || !term || settings.dictionary.includes(term)) return;
    save({ ...settings, dictionary: [...settings.dictionary, term] });
    setDraft("");
  };

  const remove = (term: string) => {
    if (!settings) return;
    save({
      ...settings,
      dictionary: settings.dictionary.filter((t) => t !== term),
    });
  };

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3">
      <h2 className="text-base font-semibold">Custom dictionary</h2>
      <p className="text-xs text-neutral-500 dark:text-neutral-400">
        Names, jargon, and spellings the cleanup pass should get right — e.g.
        "Vespry", "Tauri", "kubectl".
      </p>
      <div className="flex gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="Add a term…"
          className="flex-1 rounded-lg border border-black/10 bg-white px-3 py-2 text-sm outline-none focus:border-blue-400 dark:border-white/10 dark:bg-white/10"
        />
        <button
          onClick={add}
          className="rounded-md bg-blue-500 px-4 py-2 text-sm font-medium text-white hover:bg-blue-600"
        >
          Add
        </button>
      </div>
      <div className="flex flex-wrap gap-2">
        {settings?.dictionary.map((term) => (
          <span
            key={term}
            className="flex items-center gap-1.5 rounded-full border border-black/10 bg-white/60 py-1 pl-3 pr-1.5 text-sm dark:border-white/10 dark:bg-white/10"
          >
            {term}
            <button
              onClick={() => remove(term)}
              className="flex h-5 w-5 items-center justify-center rounded-full text-neutral-400 hover:bg-black/10 hover:text-red-500 dark:hover:bg-white/10"
            >
              ×
            </button>
          </span>
        ))}
        {settings && settings.dictionary.length === 0 && (
          <p className="py-6 text-sm text-neutral-400">No terms yet.</p>
        )}
      </div>
    </div>
  );
}
