// 电脑声音来自哪个软件：全部 / 只听某个 App
import { useEffect, useRef, useState } from "react";
import { api, type AudioApp } from "../lib/ipc";
import { useSettings } from "../store/settings";
import { restartIfRunning } from "../store/session";
import { IconCheck, IconChevron, IconMonitor } from "./icons";
import { Spinner, cx } from "./ui";

function AppIcon({ icon, size = 18 }: { icon?: string; size?: number }) {
  if (!icon) return <span className="inline-block rounded bg-surface-2" style={{ width: size, height: size }} />;
  return <img src={`data:image/png;base64,${icon}`} width={size} height={size} alt="" className="shrink-0" draggable={false} />;
}

export function AppPicker({ compact = false, align = "left" }: { compact?: boolean; align?: "left" | "right" }) {
  const settings = useSettings((s) => s.settings)!;
  const [open, setOpen] = useState(false);
  const [apps, setApps] = useState<AudioApp[] | null>(null);
  const [loading, setLoading] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const selected = settings.systemApp;

  const load = async () => {
    setLoading(true);
    try {
      setApps(await api.listAudioApps());
    } catch {
      setApps([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    void load();
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  const choose = async (app: AudioApp | null) => {
    setOpen(false);
    const { settings: cur, saveNow } = useSettings.getState();
    if (!cur) return;
    await saveNow({ ...cur, systemApp: app?.bundleId ?? "", systemAppName: app?.name ?? "" });
    await restartIfRunning();
  };

  const selectedApp = apps?.find((a) => a.bundleId === selected);
  const label = selected ? settings.systemAppName || selected : "所有电脑声音";

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        title="电脑声音来自哪个软件"
        className={cx(
          "inline-flex max-w-[220px] items-center gap-1.5 rounded-lg border border-line bg-bg text-fg transition hover:border-fg-3",
          compact ? "h-8 px-2.5 text-[13px]" : "h-9 px-3 text-[13px]",
        )}
      >
        {selected ? <AppIcon icon={selectedApp?.icon} size={16} /> : <IconMonitor size={15} className="text-fg-2" />}
        <span className="truncate">{label}</span>
        <IconChevron size={13} className="shrink-0 rotate-90 text-fg-3" />
      </button>

      {open && (
        <div
          className={cx(
            "absolute top-full z-30 mt-1.5 w-[300px] overflow-hidden rounded-xl border border-line bg-bg shadow-lg",
            align === "left" ? "left-0" : "right-0",
          )}
        >
          <div className="px-3.5 pb-1.5 pt-3 text-xs font-medium text-fg-3">听哪个软件的声音</div>
          <Option active={!selected} onClick={() => void choose(null)} icon={<IconMonitor size={16} className="text-fg-2" />} name="所有电脑声音" />
          <div className="my-1 border-t border-line" />
          <div className="max-h-[280px] overflow-y-auto">
            {selected && !selectedApp && !loading && (
              <Option active icon={<AppIcon />} name={settings.systemAppName || selected} note="未运行" onClick={() => setOpen(false)} />
            )}
            {loading && !apps && (
              <div className="flex items-center gap-2 px-3.5 py-2 text-[13px] text-fg-3">
                <Spinner size={13} />
                正在查找…
              </div>
            )}
            {apps?.map((a) => (
              <Option
                key={a.bundleId}
                active={a.bundleId === selected}
                icon={<AppIcon icon={a.icon} />}
                name={a.name}
                playing={a.playing}
                onClick={() => void choose(a)}
              />
            ))}
          </div>
          <div className="flex items-start justify-between gap-2 border-t border-line px-3.5 py-2">
            <span className="text-[11px] leading-relaxed text-fg-3">只列出用过声音的软件。没找到的话，先在那个软件里播放一下声音再刷新。</span>
            <button className="shrink-0 text-xs text-accent hover:underline" onClick={() => void load()}>
              刷新
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function Option({
  active,
  icon,
  name,
  note,
  playing,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  name: string;
  note?: string;
  playing?: boolean;
  onClick: () => void;
}) {
  return (
    <button className="flex w-full items-center gap-2.5 px-3.5 py-1.5 text-left hover:bg-surface-2" onClick={onClick}>
      {icon}
      <span className="min-w-0 flex-1 truncate text-[13px] text-fg">{name}</span>
      {playing && (
        <span className="flex shrink-0 items-center gap-1 text-[11px] text-ok">
          <span className="h-1.5 w-1.5 rounded-full bg-ok" />
          正在播放
        </span>
      )}
      {note && <span className="shrink-0 text-[11px] text-fg-3">{note}</span>}
      <IconCheck size={14} className={cx("shrink-0 text-accent", !active && "invisible")} />
    </button>
  );
}
