// 悬浮字幕窗口：透明背景、最近几句，整块可拖动
import { useEffect } from "react";
import { api, formatHotkey } from "../../lib/ipc";
import { cx } from "../../components/ui";
import { IconCursorOff, IconPin, IconX } from "../../components/icons";
import { syncSettingsAcrossWindows, useSettings } from "../../store/settings";
import { initSessionListeners, isMeeting, useSession, type Item } from "../../store/session";

const SHADOW = { textShadow: "0 1px 3px rgba(0,0,0,.55)" };

export function SubtitleWindow() {
  const settings = useSettings((s) => s.settings);
  const items = useSession((s) => s.items);
  const state = useSession((s) => s.state);
  const clickThrough = useSession((s) => s.ui.clickThrough);

  useEffect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    useSession.setState({ maxItems: 8 });
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await useSettings.getState().load();
      const un = [await initSessionListeners(false), await syncSettingsAcrossWindows()];
      if (cancelled) return un.forEach((f) => f());
      cleanups.push(...un);
    })();
    return () => {
      cancelled = true;
      cleanups.forEach((f) => f());
    };
  }, []);

  if (!settings) return null;
  const cfg = settings.subtitle;
  const show = settings.translate.enabled ? cfg.show : "original";
  const recent = items.filter((i) => i.text).slice(-Math.max(1, cfg.lines));
  const meeting = isMeeting(items);

  return (
    <div
      data-tauri-drag-region
      className="group relative flex h-full w-full flex-col justify-end gap-2.5 overflow-hidden rounded-2xl px-6 py-4"
      style={{ background: `rgba(18,19,23,${cfg.opacity})` }}
    >
      {recent.length === 0 ? (
        <p data-tauri-drag-region className="text-center text-[14px] text-white/55" style={SHADOW}>
          {state === "running" ? "正在听…" : "听译字幕：点「开始」后，译文会显示在这里"}
        </p>
      ) : (
        recent.map((it) => <Line key={it.id} item={it} show={show} size={cfg.fontSize} meeting={meeting} />)
      )}

      {/* 鼠标放上去时显示：置顶开关、隐藏。鼠标穿透开关不放在这里，避免拖动时误点后字幕再也点不动 */}
      <div className="absolute right-2 top-2 hidden gap-0.5 group-hover:flex">
        <HoverButton
          label={cfg.alwaysOnTop ? "取消置顶" : "始终置顶"}
          active={cfg.alwaysOnTop}
          onClick={() => useSettings.getState().update((s) => ({ ...s, subtitle: { ...s.subtitle, alwaysOnTop: !s.subtitle.alwaysOnTop } }))}
        >
          <IconPin size={14} filled={cfg.alwaysOnTop} />
        </HoverButton>
        <HoverButton label="隐藏字幕" onClick={() => void api.setSubtitleVisible(false)}>
          <IconX size={14} />
        </HoverButton>
      </div>

      {clickThrough && (
        <div className="pointer-events-none absolute left-3 top-2 flex items-center gap-1 text-[11px] text-white/45" style={SHADOW}>
          <IconCursorOff size={12} />
          鼠标穿透中{settings.hotkeys.clickThrough ? `，按 ${formatHotkey(settings.hotkeys.clickThrough)} 关闭` : ""}
        </div>
      )}
    </div>
  );
}

function Line({ item, show, size, meeting }: { item: Item; show: "both" | "translation" | "original"; size: number; meeting: boolean }) {
  const small = Math.max(12, Math.round(size * 0.62));
  const who = meeting ? (item.channel === "mic" ? "我：" : "对方：") : "";
  const original = (big: boolean) => (
    <p
      data-tauri-drag-region
      className={cx("leading-snug", big ? "font-semibold text-white" : "text-white/70")}
      style={{ ...SHADOW, fontSize: big ? size : small }}
    >
      {who}
      {item.text}
    </p>
  );

  if (show === "original") return <div data-tauri-drag-region>{original(true)}</div>;
  return (
    <div data-tauri-drag-region className="flex flex-col gap-0.5">
      {show === "both" && original(false)}
      {item.translation ? (
        <p data-tauri-drag-region className="font-semibold leading-snug text-white" style={{ ...SHADOW, fontSize: size }}>
          {show === "translation" && who}
          {item.translation}
        </p>
      ) : show === "translation" ? (
        // 只显示译文时，译文出来之前先用小字显示原文
        original(false)
      ) : null}
    </div>
  );
}

function HoverButton({ label, onClick, active, children }: { label: string; onClick: () => void; active?: boolean; children: React.ReactNode }) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={onClick}
      className={cx("rounded-md p-1.5 hover:bg-white/15 hover:text-white", active ? "text-white" : "text-white/60")}
    >
      {children}
    </button>
  );
}
