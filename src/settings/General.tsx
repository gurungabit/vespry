import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Status = {
  microphone: boolean;
  accessibility: boolean;
  modelInstalled: boolean;
};

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

  const refresh = () => {
    invoke<Status>("get_status").then(setStatus).catch(console.error);
  };

  useEffect(() => {
    refresh();
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

  const request = (name: string) =>
    invoke("request_permission", { name }).then(refresh);

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3">
      <h2 className="text-base font-semibold">Setup</h2>
      {status && (
        <>
          <Row
            ok={status.microphone}
            label="Microphone"
            detail="Needed to hear you dictate"
            action="Grant"
            onAction={() => request("microphone")}
          />
          <Row
            ok={status.accessibility}
            label="Accessibility"
            detail="Needed for the hotkey and to type into other apps"
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

      <h2 className="mt-4 text-base font-semibold">How to dictate</h2>
      <div className="rounded-lg border border-black/10 bg-white/60 px-4 py-3 text-sm leading-6 dark:border-white/10 dark:bg-white/5">
        <p>
          <b>Hold right ⌘</b> and speak, release to insert the text wherever
          your cursor is.
        </p>
        <p className="text-neutral-500 dark:text-neutral-400">
          Quick-tap instead to dictate hands-free — tap again to finish.
        </p>
      </div>

      <h2 className="mt-4 text-base font-semibold">Try it</h2>
      <textarea
        className="min-h-24 w-full resize-none rounded-lg border border-black/10 bg-white p-3 text-sm outline-none focus:border-blue-400 dark:border-white/10 dark:bg-white/10"
        placeholder="Click here, then hold right ⌘ and speak…"
      />
    </div>
  );
}
