import { useState } from "react";
import General from "./settings/General";
import Models from "./settings/Models";
import Dictionary from "./settings/Dictionary";
import History from "./settings/History";
import About from "./settings/About";

const TABS = ["General", "Models", "Dictionary", "History", "About"] as const;
type Tab = (typeof TABS)[number];

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
        {tab === "General" ? (
          <General />
        ) : tab === "Models" ? (
          <Models />
        ) : tab === "Dictionary" ? (
          <Dictionary />
        ) : tab === "History" ? (
          <History />
        ) : (
          <About />
        )}
      </main>
    </div>
  );
}
