import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type HistoryEntry = {
  at: number;
  raw: string;
  cleaned: string | null;
};

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<number | null>(null);

  const refresh = () =>
    invoke<HistoryEntry[]>("get_history").then(setEntries).catch(console.error);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 4000);
    return () => clearInterval(interval);
  }, []);

  const copy = (entry: HistoryEntry) => {
    navigator.clipboard.writeText(entry.cleaned ?? entry.raw);
    setCopied(entry.at);
    setTimeout(() => setCopied(null), 1200);
  };

  const remove = (at: number) =>
    invoke("delete_history_entry", { at }).then(refresh).catch(console.error);

  const q = query.toLowerCase();
  const filtered = entries.filter(
    (e) =>
      e.raw.toLowerCase().includes(q) ||
      (e.cleaned ?? "").toLowerCase().includes(q),
  );

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3">
      <div className="flex items-center gap-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search history…"
          className="flex-1 rounded-lg border border-black/10 bg-white px-3 py-2 text-sm outline-none focus:border-blue-400 dark:border-white/10 dark:bg-white/10"
        />
        {entries.length > 0 && (
          <button
            onClick={() =>
              invoke("clear_history").then(refresh).catch(console.error)
            }
            className="rounded-md px-3 py-2 text-xs text-red-500 hover:bg-red-500/10"
          >
            Clear all
          </button>
        )}
      </div>
      {filtered.length === 0 && (
        <p className="py-10 text-center text-sm text-neutral-400">
          {entries.length === 0
            ? "Dictations will appear here."
            : "No matches."}
        </p>
      )}
      {filtered.map((e) => (
        <div
          key={e.at}
          className="group rounded-lg border border-black/10 bg-white/60 px-4 py-3 dark:border-white/10 dark:bg-white/5"
        >
          <div className="flex items-start justify-between gap-3">
            <p className="text-sm leading-6">{e.cleaned ?? e.raw}</p>
            <div className="flex shrink-0 gap-1 opacity-0 transition-opacity group-hover:opacity-100">
              <button
                onClick={() => copy(e)}
                className="rounded-md px-2 py-1 text-xs text-blue-500 hover:bg-blue-500/10"
              >
                {copied === e.at ? "Copied" : "Copy"}
              </button>
              <button
                onClick={() => remove(e.at)}
                className="rounded-md px-2 py-1 text-xs text-red-500 hover:bg-red-500/10"
              >
                Delete
              </button>
            </div>
          </div>
          <div className="mt-1 flex items-center gap-2 text-xs text-neutral-400">
            <span>{new Date(e.at).toLocaleString()}</span>
            {e.cleaned && (
              <span title={`Raw: ${e.raw}`} className="cursor-help underline decoration-dotted">
                cleaned
              </span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
