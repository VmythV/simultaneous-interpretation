// 主窗口顶栏：状态、听什么、译成什么、音量、开始/停止、字幕、历史、设置
import { api, formatHotkey, type Channel, type Source } from "../../lib/ipc";
import { Button, IconButton, LangSelect, SOURCE_LABEL, Select, Spinner, cx } from "../../components/ui";
import { IconGear, IconHistory, IconStop, IconSubtitle } from "../../components/icons";
import { useSettings, withTarget } from "../../store/settings";
import { changeSource, startListening, stopListening, useSession } from "../../store/session";
import { AppPicker } from "../../components/AppPicker";

export function TopBar({ minimal }: { minimal: boolean }) {
  const settings = useSettings((s) => s.settings)!;
  const presets = useSettings((s) => s.presets)!;
  const state = useSession((s) => s.state);
  const runningSource = useSession((s) => s.source);
  const subtitleVisible = useSession((s) => s.ui.subtitleVisible);
  const running = state === "running";
  const source: Source = running && runningSource ? runningSource : settings.source;
  const subtitleKey = formatHotkey(settings.hotkeys.subtitle);

  return (
    <header data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-2.5 pl-[84px] pr-3">
      <Status />
      {!minimal && (
        <>
          <div className="ml-2 h-4 w-px bg-line" />
          <span className="text-[13px] text-fg-3">听</span>
          <Select
            compact
            className="w-[104px]"
            value={source}
            onChange={(v) => void changeSource(v)}
            options={(["mic", "system", "both"] as Source[]).map((v) => ({ value: v, label: SOURCE_LABEL[v] }))}
          />
          {source !== "mic" && <AppPicker compact />}
          <span className="ml-1 text-[13px] text-fg-3">译成</span>
          <LangSelect
            compact
            className="w-[96px]"
            value={settings.targetLang}
            languages={presets.languages}
            onChange={(v) => useSettings.getState().update((s) => withTarget(s, v))}
          />
        </>
      )}
      <div data-tauri-drag-region className="h-full flex-1" />
      {running && <Meters source={source} />}
      {!minimal && <StartStop />}
      <IconButton
        label={`悬浮字幕${subtitleKey ? `（${subtitleKey}）` : ""}`}
        onClick={() => void api.setSubtitleVisible(!subtitleVisible)}
        className={cx(subtitleVisible && "bg-accent-soft text-accent hover:bg-accent-soft hover:text-accent")}
      >
        <IconSubtitle size={17} />
      </IconButton>
      <IconButton label="历史记录" onClick={() => void api.openHistory()}>
        <IconHistory size={17} />
      </IconButton>
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

function Meters({ source }: { source: Source }) {
  if (source === "both")
    return (
      <div className="mr-1 flex items-center gap-2.5">
        <LevelMeter channel="mic" label="我" />
        <LevelMeter channel="system" label="对方" />
      </div>
    );
  return <LevelMeter channel={source} />;
}

/** 5 格音量条（对数刻度 -60dB ~ -10dB） */
function LevelMeter({ channel, label }: { channel: Channel; label?: string }) {
  const rms = useSession((s) => s.level[channel]);
  const speaking = useSession((s) => s.speaking[channel]);
  const db = 20 * Math.log10(Math.max(rms, 1e-6));
  const lit = Math.round(Math.min(Math.max((db + 60) / 50, 0), 1) * 5);
  return (
    <div className="flex items-center gap-1.5" title={label ? `${label}的音量` : "音量"}>
      {label && <span className="text-[11px] text-fg-3">{label}</span>}
      <div className="flex h-4 items-end gap-[3px]">
        {[0, 1, 2, 3, 4].map((i) => (
          <span
            key={i}
            className={cx("w-[3px] rounded-full transition-colors duration-100", i < lit ? (speaking ? "bg-accent" : "bg-fg-3") : "bg-surface-2")}
            style={{ height: `${6 + i * 2.5}px` }}
          />
        ))}
      </div>
    </div>
  );
}
