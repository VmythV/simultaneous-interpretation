// 首次启动向导：语音识别 → 翻译 → 默认设置
import { Fragment, useState } from "react";
import { Button, cx } from "../../components/ui";
import { IconCheck, Logo } from "../../components/icons";
import { BasicsForm } from "../../forms/GeneralForm";
import { LlmForm } from "../../forms/LlmForm";
import { SttForm } from "../../forms/SttForm";
import type { Settings } from "../../lib/ipc";
import { useSettings } from "../../store/settings";

const STEPS = [
  { name: "语音识别", title: "第 1 步：语音识别", desc: "把声音转成文字的服务。推荐阿里云百炼：中文效果好，边说边出字。" },
  { name: "翻译", title: "第 2 步：翻译", desc: "把识别出的文字翻译成目标语言的大模型服务（兼容 OpenAI 协议）。" },
  { name: "默认设置", title: "第 3 步：默认设置", desc: "以后打开软件就按这里的设置开始。主界面上也可以随时修改。" },
];

function sttReady(d: Settings) {
  return !!(d.stt.apiKey && d.stt.baseUrl && d.stt.model);
}

function llmReady(d: Settings) {
  const t = d.translate;
  if (!t.enabled) return true;
  const sharing = d.stt.preset === "aliyun" && t.useSttKey && t.preset === "aliyun";
  return !!((sharing ? d.stt.apiKey : t.apiKey) && t.baseUrl && t.model);
}

export function Onboarding() {
  const settings = useSettings((s) => s.settings)!;
  const presets = useSettings((s) => s.presets)!;
  const saveNow = useSettings((s) => s.saveNow);
  const [draft, setDraft] = useState<Settings>(settings);
  const [step, setStep] = useState(0);

  const canNext = step === 0 ? sttReady(draft) : step === 1 ? llmReady(draft) : true;
  const next = async (d: Settings = draft) => {
    if (step < 2) {
      await saveNow(d); // 中途退出也不丢已填的内容
      setStep(step + 1);
    } else {
      await saveNow({ ...d, onboarded: true });
    }
  };
  const skipTranslate = () => {
    const d = { ...draft, translate: { ...draft.translate, enabled: false } };
    setDraft(d);
    void next(d);
  };

  return (
    <div className="flex h-full flex-col bg-bg">
      <div data-tauri-drag-region className="h-[52px] shrink-0" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[560px] px-8 pb-10">
          {step === 0 && (
            <div className="mb-7 flex items-center gap-3.5">
              <Logo size={44} />
              <div>
                <h1 className="text-[20px] font-semibold text-fg">欢迎使用听译</h1>
                <p className="mt-0.5 text-[13px] text-fg-2">实时语音转文字 + 翻译。先花一分钟配置服务，之后打开就能用。</p>
              </div>
            </div>
          )}
          <Stepper current={step} />
          <h2 className="mt-7 text-[17px] font-semibold text-fg">{STEPS[step].title}</h2>
          <p className="mb-6 mt-1 text-[13px] text-fg-2">{STEPS[step].desc}</p>
          {step === 0 && <SttForm draft={draft} onChange={setDraft} presets={presets} />}
          {step === 1 && <LlmForm draft={{ ...draft, translate: { ...draft.translate, enabled: true } }} onChange={setDraft} presets={presets} />}
          {step === 2 && <BasicsForm draft={draft} onChange={setDraft} presets={presets} />}
        </div>
      </div>
      <footer className="flex h-16 shrink-0 items-center justify-between border-t border-line px-8">
        <Button variant="ghost" onClick={() => setStep(step - 1)} disabled={step === 0}>
          上一步
        </Button>
        <div className="flex items-center gap-3">
          {step === 1 && (
            <Button variant="ghost" onClick={skipTranslate}>
              暂不翻译，只转文字
            </Button>
          )}
          {!canNext && <span className="text-xs text-fg-3">请先填写 API Key</span>}
          <Button variant="primary" onClick={() => void next()} disabled={!canNext}>
            {step === 2 ? "完成，开始使用" : "下一步"}
          </Button>
        </div>
      </footer>
    </div>
  );
}

function Stepper({ current }: { current: number }) {
  return (
    <div className="flex items-center gap-2">
      {STEPS.map((s, i) => (
        <Fragment key={s.name}>
          {i > 0 && <div className={cx("h-px flex-1", i <= current ? "bg-accent" : "bg-line")} />}
          <div className="flex items-center gap-2">
            <span
              className={cx(
                "inline-flex h-6 w-6 items-center justify-center rounded-full text-[12px] font-semibold",
                i < current ? "bg-accent text-on-accent" : i === current ? "bg-accent-soft text-accent ring-1 ring-accent" : "bg-surface-2 text-fg-3",
              )}
            >
              {i < current ? <IconCheck size={13} /> : i + 1}
            </span>
            <span className={cx("text-[13px]", i === current ? "font-medium text-fg" : "text-fg-2")}>{s.name}</span>
          </div>
        </Fragment>
      ))}
    </div>
  );
}
