// 通用设置：听什么、译成什么、断句参数
import { useEffect, useState } from "react";
import { api, type InputDevice, type Presets, type Settings } from "../lib/ipc";
import { Collapsible, Field, LangSelect, Select, Slider, SourcePicker, Switch } from "../components/ui";
import { withTarget } from "../store/settings";

/** 听什么 + 译成什么（向导第 3 步、设置页"通用"共用） */
export function BasicsForm({ draft, onChange, presets }: { draft: Settings; onChange: (s: Settings) => void; presets: Presets }) {
  return (
    <div className="flex flex-col gap-6">
      <Field label="默认听" hint={draft.source === "mic" ? "麦克风：适合自己说话、现场对话" : "电脑声音：适合视频、网课、线上会议"}>
        <div>
          <SourcePicker value={draft.source} onChange={(source) => onChange({ ...draft, source })} />
        </div>
      </Field>
      <Field label="译成">
        <LangSelect value={draft.targetLang} onChange={(targetLang) => onChange(withTarget(draft, targetLang))} languages={presets.languages} className="w-56" />
      </Field>
      <Field label="原文已经是这个语言时，译成" hint="例如目标是中文：听到英文译成中文，听到中文就译成这里选的语言">
        <LangSelect
          value={draft.fallbackLang}
          onChange={(fallbackLang) => onChange({ ...draft, fallbackLang })}
          languages={presets.languages.filter((l) => l.code !== draft.targetLang)}
          className="w-56"
        />
      </Field>
    </div>
  );
}

export function GeneralForm({ draft, onChange, presets }: { draft: Settings; onChange: (s: Settings) => void; presets: Presets }) {
  const [devices, setDevices] = useState<InputDevice[]>([]);
  useEffect(() => {
    api.listInputDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

  return (
    <div className="flex flex-col gap-6">
      <BasicsForm draft={draft} onChange={onChange} presets={presets} />

      <Field label="麦克风">
        <Select
          className="w-72"
          value={draft.micDevice}
          onChange={(micDevice) => onChange({ ...draft, micDevice })}
          options={[{ value: "", label: "系统默认" }, ...devices.map((d) => ({ value: d.id, label: d.name }))]}
        />
      </Field>

      <Switch checked={draft.autoStart} onChange={(autoStart) => onChange({ ...draft, autoStart })} label="打开软件后自动开始" hint="省去每次点「开始」" />

      <Collapsible>
        <Field label="断句停顿时长" hint="停顿超过这个时长就算一句话说完。越短出字越快，但可能把一句话切成两半。">
          <Slider value={draft.silenceMs} onChange={(silenceMs) => onChange({ ...draft, silenceMs })} min={300} max={1000} step={50} format={(v) => `${v} 毫秒`} />
        </Field>
        <Field label="单句最长时长" hint="一直说不停顿时，到这个时长会强制断句">
          <Slider value={draft.maxSegmentSec} onChange={(maxSegmentSec) => onChange({ ...draft, maxSegmentSec })} min={5} max={30} step={1} format={(v) => `${v} 秒`} />
        </Field>
        <Field label="人声检测灵敏度" hint="环境嘈杂、经常误识别时调高；小声说话识别不到时调低">
          <Slider
            value={draft.vadThreshold}
            onChange={(vadThreshold) => onChange({ ...draft, vadThreshold })}
            min={0.3}
            max={0.8}
            step={0.05}
            format={(v) => (v < 0.45 ? "灵敏" : v > 0.6 ? "抗噪" : "标准")}
          />
        </Field>
      </Collapsible>
    </div>
  );
}
