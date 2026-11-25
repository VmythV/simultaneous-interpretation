// 设置页"字幕"：悬浮字幕的开关、穿透、样式
import { api, type Settings, type SubtitleSettings, type SubtitleShow, type UiState } from "../lib/ipc";
import { Field, Select, Slider, Switch } from "../components/ui";

export function SubtitleForm({ draft, onChange, ui }: { draft: Settings; onChange: (s: Settings) => void; ui: UiState }) {
  const s = draft.subtitle;
  const set = (patch: Partial<SubtitleSettings>) => onChange({ ...draft, subtitle: { ...s, ...patch } });
  return (
    <div className="flex flex-col gap-6">
      <Switch
        checked={ui.subtitleVisible}
        onChange={(v) => void api.setSubtitleVisible(v)}
        label="显示悬浮字幕"
        hint="始终显示在其他窗口上面。拖动字幕可以移动位置，拖动边缘可以调整大小"
      />
      <Switch
        checked={ui.clickThrough}
        onChange={(v) => void api.setClickThrough(v)}
        label="鼠标穿透"
        hint="开启后点击会穿过字幕，落到下面的窗口上（此时不能拖动字幕）。可以用快捷键或菜单栏关闭"
      />
      <div className="border-t border-line" />
      <Field label="显示内容">
        <Select<SubtitleShow>
          className="w-56"
          value={s.show}
          onChange={(show) => set({ show })}
          options={[
            { value: "both", label: "原文 + 译文" },
            { value: "translation", label: "只显示译文" },
            { value: "original", label: "只显示原文" },
          ]}
        />
      </Field>
      <Field label="字号" hint="原文按比例缩小显示">
        <Slider value={s.fontSize} onChange={(fontSize) => set({ fontSize })} min={16} max={48} step={1} format={(v) => `${v}`} />
      </Field>
      <Field label="背景不透明度">
        <Slider value={s.opacity} onChange={(opacity) => set({ opacity })} min={0} max={1} step={0.05} format={(v) => `${Math.round(v * 100)}%`} />
      </Field>
      <Field label="显示最近几句">
        <Slider value={s.lines} onChange={(lines) => set({ lines })} min={1} max={4} step={1} format={(v) => `${v} 句`} />
      </Field>
    </div>
  );
}
