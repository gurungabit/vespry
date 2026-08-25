import { useState } from "react";

const TABS = ["General", "Models", "Dictionary", "History", "About"] as const;
type Tab = (typeof TABS)[number];

const TAB_ICONS: Record<Tab, string> = {
  General: "⚙︎",
  Models: "⬇︎",
  Dictionary: "📖",
  History: "🕘",
  About: "ℹ︎",
};

function Placeholder({ tab }: { tab: Tab }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-neutral-400">
      <span className="text-3xl">{TAB_ICONS[tab]}</span>
      <p className="text-sm">{tab} — coming soon</p>
    </div>
  );
}

export default function App() {
  const [tab, setTab] = useState<Tab>("General");

  return (
    <div className="flex h-screen bg-neutral-100 text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <aside className="flex w-44 shrink-0 flex-col gap-1 border-r border-black/10 bg-neutral-200/60 p-3 pt-8 dark:border-white/10 dark:bg-neutral-800/60">
        <h1 className="mb-2 px-2 text-lg font-semibold tracking-tight">Vespry</h1>
        {TABS.map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`rounded-md px-2 py-1.5 text-left text-sm transition-colors ${
              tab === t
                ? "bg-blue-500 text-white"
                : "hover:bg-black/5 dark:hover:bg-white/10"
            }`}
          >
            {t}
          </button>
        ))}
      </aside>
      <main className="flex-1 overflow-y-auto p-6">
        <Placeholder tab={tab} />
      </main>
    </div>
  );
}
