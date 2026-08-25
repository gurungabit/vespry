export default function About() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <div className="flex h-20 w-20 items-end justify-center gap-[3px] rounded-[22px] bg-gradient-to-br from-blue-500 to-violet-600 pb-[26px] pt-[26px]">
        {[10, 22, 34, 22, 10].map((h, i) => (
          <div
            key={i}
            className="w-[4px] rounded-full bg-white"
            style={{ height: h, alignSelf: "center" }}
          />
        ))}
      </div>
      <div>
        <h2 className="text-lg font-semibold">Vespry</h2>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          Local, private dictation · v0.1.0
        </p>
      </div>
      <p className="max-w-sm text-xs leading-5 text-neutral-400">
        Speech recognition by NVIDIA Parakeet and whisper.cpp, cleanup by
        Qwen3 via llama.cpp — everything runs on your Mac. No audio or text
        ever leaves your machine.
      </p>
    </div>
  );
}
