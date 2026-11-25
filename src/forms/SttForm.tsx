// 语音识别服务配置（向导第 1 步、设置页"语音识别"共用）
import { api, type Presets, type Settings, type SttPreset, type SttSettings } from "../lib/ipc";
import { ChoiceCards, Collapsible, ExternalLink, Field, KeyInput, Select, TestButton, TextInput } from "../components/ui";

const STT_LANGUAGES = [
  { value: "auto", label: "自动（中英混合）" },
  { value: "zh", label: "中文" },
  { value: "en", label: "英文" },
  { value: "ja", label: "日文" },
  { value: "ko", label: "韩文" },
  { value: "yue", label: "粤语" },
];

export function SttForm({ draft, onChange, presets }: { draft: Settings; onChange: (s: Settings) => void; presets: Presets }) {
  const stt = draft.stt;
  const preset = presets.stt.find((p) => p.id === stt.preset) ?? presets.stt[presets.stt.length - 1];
  const isCustom = preset.id === "custom";
  const set = (patch: Partial<SttSettings>) => onChange({ ...draft, stt: { ...stt, ...patch } });

  const choose = (p: SttPreset) => {
    const next: Settings = { ...draft, stt: { ...stt, preset: p.id, mode: p.mode, baseUrl: p.baseUrl, model: p.model } };
    // 识别不再是百炼时，翻译不能再共用识别的 Key
    if (p.id !== "aliyun" && draft.translate.useSttKey) {
      next.translate = { ...draft.translate, useSttKey: false };
    }
    onChange(next);
  };

  const urlAndModel = (
    <>
      <Field label="服务地址（Base URL）" hint={stt.mode === "aliyun-realtime" ? "WebSocket 地址" : "包含 /v1，例如 https://example.com/v1"}>
        <TextInput value={stt.baseUrl} onChange={(v) => set({ baseUrl: v.trim() })} placeholder="https://…/v1" mono />
      </Field>
      <Field label="模型">
        <TextInput value={stt.model} onChange={(v) => set({ model: v.trim() })} list={preset.models} placeholder="模型名称" mono />
      </Field>
    </>
  );

  return (
    <div className="flex flex-col gap-6">
      <Field label="服务商">
        <ChoiceCards items={presets.stt} value={stt.preset} onChange={choose} />
      </Field>

      <Field label="API Key" hint={preset.note || undefined}>
        <KeyInput value={stt.apiKey} onChange={(v) => set({ apiKey: v })} />
        {preset.keyUrl && (
          <div>
            <ExternalLink href={preset.keyUrl}>去 {preset.name} 获取 API Key</ExternalLink>
          </div>
        )}
      </Field>

      {isCustom && urlAndModel}

      <TestButton run={() => api.testStt(draft)} okLabel="识别成功" disabled={!stt.apiKey || !stt.baseUrl || !stt.model} />

      <Collapsible>
        <Field label="识别方式">
          <div className="text-[13px] text-fg-2">
            {stt.mode === "aliyun-realtime" ? "流式：边说边出字，延迟最低" : "分段：一句话说完后整句上传识别"}
          </div>
        </Field>
        {!isCustom && urlAndModel}
        <Field label="识别语言" hint="说的语言比较固定时，指定语言可以提高准确率">
          <Select value={stt.language} onChange={(v) => set({ language: v })} options={STT_LANGUAGES} />
        </Field>
      </Collapsible>
    </div>
  );
}
