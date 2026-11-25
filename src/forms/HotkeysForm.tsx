// 设置页"快捷键"：全局快捷键（在任何软件里都有效）
import { useEffect, useState } from "react";
import { api, formatHotkey, type HotkeySettings, type Settings } from "../lib/ipc";
import { Button, HotkeyInput } from "../components/ui";
import { IconAlert } from "../components/icons";
import { useSettings } from "../store/settings";

const ROWS: { key: keyof HotkeySettings; label: string; hint: string }[] = [
  { key: "toggle", label: "开始 / 停止", hint: "在任何软件里都能开始或停止识别" },
  { key: "subtitle", label: "显示 / 隐藏悬浮字幕", hint: "" },
  { key: "clickThrough", label: "字幕鼠标穿透", hint: "开启后可以点击字幕下面的内容" },
];

const DEFAULTS: HotkeySettings = { toggle: "Ctrl+Alt+S", subtitle: "Ctrl+Alt+Z", clickThrough: "Ctrl+Alt+X" };

export function HotkeysForm({ draft, onChange }: { draft: Settings; onChange: (s: Settings) => void }) {
  const warnings = useSettings((s) => s.warnings);
  const savedAt = useSettings((s) => s.savedAt);
  const [errors, setErrors] = useState<string[]>([]);
  const h = draft.hotkeys;

  useEffect(() => {
    api.getHotkeyErrors().then(setErrors).catch(() => {});
  }, []);
  useEffect(() => {
    if (savedAt) setErrors(warnings);
  }, [warnings, savedAt]);

  const values = Object.values(h).filter(Boolean);
  const duplicate = values.length !== new Set(values).size;

  return (
    <div className="flex flex-col gap-5">
      <p className="text-[13px] leading-relaxed text-fg-2">
        点击输入框，然后按下新的组合键。需要包含 ⌃ Control、⌥ Option、⌘ Command 中的至少一个；按 Delete 清除，Esc 取消。
      </p>
      {ROWS.map((r) => (
        <div key={r.key} className="flex items-center justify-between gap-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-[13px] font-medium text-fg">{r.label}</span>
            {r.hint && <span className="text-xs text-fg-3">{r.hint}</span>}
          </div>
          <HotkeyInput
            value={h[r.key]}
            onChange={(v) => onChange({ ...draft, hotkeys: { ...h, [r.key]: v } })}
            onRecording={(on) => void api.suspendHotkeys(on)}
          />
        </div>
      ))}

      {(duplicate || errors.length > 0) && (
        <div className="flex flex-col gap-1.5 rounded-lg bg-danger-soft px-3.5 py-2.5 text-[13px] text-danger">
          {duplicate && (
            <div className="flex gap-2">
              <IconAlert size={15} className="mt-0.5 shrink-0" />
              有两个功能用了同一个快捷键
            </div>
          )}
          {errors.map((e) => (
            <div key={e} className="flex gap-2">
              <IconAlert size={15} className="mt-0.5 shrink-0" />
              {e}
            </div>
          ))}
        </div>
      )}

      <div>
        <Button size="sm" variant="ghost" onClick={() => onChange({ ...draft, hotkeys: DEFAULTS })}>
          恢复默认（{formatHotkey(DEFAULTS.toggle)}、{formatHotkey(DEFAULTS.subtitle)}、{formatHotkey(DEFAULTS.clickThrough)}）
        </Button>
      </div>
    </div>
  );
}
