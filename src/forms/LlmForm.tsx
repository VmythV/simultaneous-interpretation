// 翻译服务配置（向导第 2 步、设置页"翻译"共用）
import { api, type LlmPreset, type LlmSettings, type Presets, type Settings } from "../lib/ipc";
import { ChoiceCards, Collapsible, ExternalLink, Field, KeyInput, Slider, Switch, TestButton, TextArea, TextInput } from "../components/ui";

export function LlmForm({ draft, onChange, presets }: { draft: Settings; onChange: (s: Settings) => void; presets: Presets }) {
  const t = draft.translate;
  const aliyun = presets.llm.find((p) => p.id === "aliyun")!;
  const canShare = draft.stt.preset === "aliyun";
  const sharing = canShare && t.useSttKey && t.preset === "aliyun";
  const preset = presets.llm.find((p) => p.id === t.preset) ?? presets.llm[presets.llm.length - 1];
  const isCustom = preset.id === "custom";
  const set = (patch: Partial<LlmSettings>) => onChange({ ...draft, translate: { ...t, ...patch } });
  const hasKey = sharing ? !!draft.stt.apiKey : !!t.apiKey;

  const choose = (p: LlmPreset) => set({ preset: p.id, baseUrl: p.baseUrl, model: p.model, useSttKey: false });

  const toggleShare = (on: boolean) =>
    on ? set({ useSttKey: true, preset: "aliyun", baseUrl: aliyun.baseUrl, model: t.preset === "aliyun" ? t.model : aliyun.model }) : set({ useSttKey: false });

  const urlAndModel = (
    <>
      <Field label="服务地址（Base URL）" hint="OpenAI 兼容地址，包含 /v1">
        <TextInput value={t.baseUrl} onChange={(v) => set({ baseUrl: v.trim() })} placeholder="https://…/v1" mono />
      </Field>
      <Field label="模型">
        <TextInput value={t.model} onChange={(v) => set({ model: v.trim() })} list={preset.models} placeholder="模型名称" mono />
      </Field>
    </>
  );

  return (
    <div className="flex flex-col gap-6">
      {canShare && (
        <Switch checked={sharing} onChange={toggleShare} label="使用阿里云百炼翻译" hint="和语音识别用同一个 Key，不用再填写" />
      )}

      {!sharing && (
        <>
          <Field label="服务商">
            <ChoiceCards items={presets.llm} value={t.preset} onChange={choose} />
          </Field>
          <Field label="API Key" hint={preset.note || undefined}>
            <KeyInput value={t.apiKey} onChange={(v) => set({ apiKey: v })} />
            {preset.keyUrl && (
              <div>
                <ExternalLink href={preset.keyUrl}>去 {preset.name} 获取 API Key</ExternalLink>
              </div>
            )}
          </Field>
          {isCustom && urlAndModel}
        </>
      )}

      <TestButton run={() => api.testTranslate(draft)} okLabel="翻译成功" disabled={!hasKey || !t.baseUrl || !t.model} />

      <Collapsible>
        {!isCustom && urlAndModel}
        <Field label="翻译风格" hint="越低越稳定、越贴近原文；越高越灵活">
          <Slider value={t.temperature} onChange={(v) => set({ temperature: v })} min={0} max={1} step={0.1} format={(v) => v.toFixed(1)} />
        </Field>
        <Field label="参考上文" hint="翻译时带上前几句，让人名、术语前后一致">
          <Slider value={t.contextSize} onChange={(v) => set({ contextSize: v })} min={0} max={8} step={1} format={(v) => (v === 0 ? "不参考" : `${v} 句`)} />
        </Field>
        <Field label="自定义提示词" hint="可以用 {target} 表示目标语言，{context} 表示上文。留空则使用下方默认提示词。">
          <TextArea value={t.prompt} onChange={(v) => set({ prompt: v })} placeholder={presets.defaultPrompt} rows={8} />
        </Field>
      </Collapsible>
    </div>
  );
}
