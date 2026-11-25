// 未开始时的主界面：一眼看懂用途 + 三步用法 + 两个选择 + 开始按钮
import { Fragment } from "react";
import { Button, LangSelect, SOURCE_HINT, SourcePicker } from "../../components/ui";
import { IconChevron, Logo } from "../../components/icons";
import { formatHotkey } from "../../lib/ipc";
import { useSettings, withTarget } from "../../store/settings";
import { startListening } from "../../store/session";

const STEPS = ["选择要听的声音", "点「开始」", "实时看原文和译文"];

export function IdleView() {
  const settings = useSettings((s) => s.settings)!;
  const presets = useSettings((s) => s.presets)!;
  const update = useSettings((s) => s.update);

  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-8 pb-14">
      <Logo size={60} />
      <h1 className="mt-5 text-[23px] font-semibold tracking-tight text-fg">实时语音转文字 + 翻译</h1>
      <p className="mt-2 text-[14px] text-fg-2">听到的话实时变成文字，并同步翻译成你需要的语言</p>

      <ol className="mt-7 flex items-center gap-2.5 text-[13px] text-fg-2">
        {STEPS.map((s, i) => (
          <Fragment key={s}>
            {i > 0 && <IconChevron size={13} className="text-fg-3" />}
            <li className="flex items-center gap-1.5">
              <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-surface-2 text-[11px] font-semibold text-fg-2">{i + 1}</span>
              {s}
            </li>
          </Fragment>
        ))}
      </ol>

      <div className="mt-9 grid grid-cols-[auto_auto] items-center gap-x-4 gap-y-3.5">
        <span className="text-right text-[14px] text-fg-2">听</span>
        <SourcePicker size="lg" value={settings.source} onChange={(source) => update((s) => ({ ...s, source }))} />
        <span className="text-right text-[14px] text-fg-2">译成</span>
        <LangSelect value={settings.targetLang} onChange={(v) => update((s) => withTarget(s, v))} languages={presets.languages} className="w-full" />
      </div>
      <p className="mt-3 max-w-[420px] text-center text-xs leading-relaxed text-fg-3">{SOURCE_HINT[settings.source]}</p>

      <Button variant="primary" size="lg" className="mt-8 w-52" onClick={() => void startListening()}>
        <span className="h-2.5 w-2.5 rounded-full bg-white" />
        开始
      </Button>
      <p className="mt-3 text-xs text-fg-3">
        快捷键 ⌘ + Enter 开始 / 停止
        {settings.hotkeys.toggle && `；在其他软件里按 ${formatHotkey(settings.hotkeys.toggle)}`}
      </p>
    </div>
  );
}
