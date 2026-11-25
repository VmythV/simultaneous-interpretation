// 主窗口顶栏：状态、听什么、译成什么、音量、开始/停止、设置
import { api, type Source } from "../../lib/ipc";
import { Button, IconButton, LangSelect, Select, Spinner, cx } from "../../components/ui";
import { IconGear, IconStop } from "../../components/icons";
import { useSettings, withTarget } from "../../store/settings";
import { restartIfRunning, startListening, stopListening, useSession } from "../../store/session";

export function TopBar({ minimal }: { minimal: boolean }) {
  const settings = useSettings((s) => s.settings)!;
  const presets = useSettings((s) => s.presets)!;
  const state = useSession((s) => s.state);
  const runningSource = useSession((s) => s.source);
  const running = state === "running";
  const source: Source = running && runningSource ? runningSource : settings.source;

  const changeSource = async (next: Source) => {
    const { saveNow } = useSettings.getState();
    await saveNow({ ...settings, source: next });
    await restartIfRunning();
  };

  return (
    <header data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-2.5 pl-[84px] pr-3">
      <Status />
      {!minimal && (
        <>
          <div className="ml-2 h-4 w-px bg-line" />
          <span className="text-[13px] text-fg-3">听</span>
          <Select
            compact
            className="w-[112px]"
            value={source}
            onChange={(v) => void changeSource(v)}
            options={[
              { value: "mic", label: "麦克风" },
              { value: "system", label: "电脑声音" },
            ]}
          />
          <span className="ml-1 text-[13px] text-fg-3">译成</span>
          <LangSelect
            compact
            className="w-[100px]"
            value={settings.targetLang}
            languages={presets.languages}
            onChange={(v) => useSettings.getState().update((s) => withTarget(s, v))}
          />
        </>
      )}
      <div data-tauri-drag-region className="h-full flex-1" />
      {running && <LevelMeter />}
      {!minimal && <StartStop />}
      <IconButton label="设置（⌘,）" onClick={() => void api.openSettings()}>
        <IconGear size={17} />
      </IconButton>
    </header>
  );
}

function Status() {
  const state = useSession((s) => s.state);
  if (state === "running")
    return (
      <div data-tauri-drag-region className="flex items-center gap-2 text-[13px] font-medium text-fg">
        <span className="rec-dot h-2 w-2 rounded-full bg-rec" />
        正在听
      </div>
    );
  if (state === "starting" || state === "stopping")
    return (
      <div data-tauri-drag-region className="flex items-center gap-2 text-[13px] text-fg-2">
        <Spinner size={13} />
        {state === "starting" ? "准备中…" : "正在停止…"}
      </div>
    );
  return (
    <div data-tauri-drag-region className="text-[14px] font-semibold text-fg">
      听译
    </div>
  );
}

function StartStop() {
  const state = useSession((s) => s.state);
  if (state === "running")
    return (
      <Button onClick={() => void stopListening()}>
        <IconStop size={13} className="text-rec" />
        停止
      </Button>
    );
  const busy = state !== "idle";
  return (
    <Button variant="primary" disabled={busy} onClick={() => void startListening()}>
      {busy ? <Spinner size={13} /> : <span className="h-2 w-2 rounded-full bg-white" />}
      开始
    </Button>
  );
}

/** 5 格音量条（对数刻度 -60dB ~ -10dB） */
function LevelMeter() {
  const source = useSession((s) => s.source) ?? "mic";
  const rms = useSession((s) => s.level[source]);
  const speaking = useSession((s) => s.speaking[source]);
  const db = 20 * Math.log10(Math.max(rms, 1e-6));
  const lit = Math.round(Math.min(Math.max((db + 60) / 50, 0), 1) * 5);
  return (
    <div className="mr-1 flex h-4 items-end gap-[3px]" title="音量">
      {[0, 1, 2, 3, 4].map((i) => (
        <span
          key={i}
          className={cx("w-[3px] rounded-full transition-colors duration-100", i < lit ? (speaking ? "bg-accent" : "bg-fg-3") : "bg-surface-2")}
          style={{ height: `${6 + i * 2.5}px` }}
        />
      ))}
    </div>
  );
}
