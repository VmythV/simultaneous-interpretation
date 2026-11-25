// 设置窗口：通用 / 语音识别 / 翻译，修改自动保存
import { useEffect, useState } from "react";
import { cx, Switch } from "../../components/ui";
import { GeneralForm } from "../../forms/GeneralForm";
import { LlmForm } from "../../forms/LlmForm";
import { SttForm } from "../../forms/SttForm";
import { api, events, type SettingsTab } from "../../lib/ipc";
import { syncSettingsAcrossWindows, useSettings } from "../../store/settings";

const TABS: { id: SettingsTab; label: string; title: string; desc: string }[] = [
  { id: "general", label: "通用", title: "通用", desc: "听什么、译成什么，以及断句方式。" },
  { id: "stt", label: "语音识别", title: "语音识别", desc: "把声音转成文字的服务。" },
  { id: "translate", label: "翻译", title: "翻译", desc: "把文字翻译成目标语言的大模型服务（兼容 OpenAI 协议）。" },
];

export function SettingsWindow({ initialTab }: { initialTab: SettingsTab }) {
  const settings = useSettings((s) => s.settings);
  const presets = useSettings((s) => s.presets);
  const update = useSettings((s) => s.update);
  const saving = useSettings((s) => s.saving);
  const savedAt = useSettings((s) => s.savedAt);
  const saveError = useSettings((s) => s.saveError);
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await useSettings.getState().load();
      setRunning((await api.getSessionState()) === "running");
      const un = [
        await syncSettingsAcrossWindows(),
        await events.settingsTab(setTab),
        await events.session((ev) => setRunning(ev.state === "running")),
      ];
      if (cancelled) return un.forEach((f) => f());
      cleanups.push(...un);
    })();
    // 关闭窗口前把未保存的修改写入
    const onUnload = () => void useSettings.getState().flush();
    window.addEventListener("beforeunload", onUnload);
    return () => {
      cancelled = true;
      cleanups.forEach((f) => f());
      window.removeEventListener("beforeunload", onUnload);
    };
  }, []);

  if (!settings || !presets) return null;
  const current = TABS.find((t) => t.id === tab) ?? TABS[0];
  const onChange = (s: typeof settings) => update(() => s);

  return (
    <div className="flex h-full bg-bg">
      <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-line bg-surface p-3">
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={cx(
              "h-8 rounded-lg px-3 text-left text-[13px] transition-colors",
              t.id === tab ? "bg-bg font-medium text-fg shadow-sm" : "text-fg-2 hover:text-fg",
            )}
          >
            {t.label}
          </button>
        ))}
        <div className="flex-1" />
        <div className="px-2 pb-1 text-xs text-fg-3">
          {saveError ? <span className="text-danger">保存失败：{saveError}</span> : saving ? "保存中…" : savedAt ? "已自动保存" : "修改会自动保存"}
        </div>
      </nav>

      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="max-w-[580px] px-8 py-7">
          <h1 className="text-[18px] font-semibold text-fg">{current.title}</h1>
          <p className="mb-6 mt-1 text-[13px] text-fg-2">{current.desc}</p>

          {running && (
            <div className="mb-6 rounded-lg bg-accent-soft px-3.5 py-2.5 text-[13px] leading-relaxed text-accent">
              正在识别中：翻译语言会立即生效，其他修改在下次「开始」时生效。
            </div>
          )}

          {tab === "general" && <GeneralForm draft={settings} onChange={onChange} presets={presets} />}
          {tab === "stt" && <SttForm draft={settings} onChange={onChange} presets={presets} />}
          {tab === "translate" && (
            <div className="flex flex-col gap-6">
              <Switch
                checked={settings.translate.enabled}
                onChange={(enabled) => onChange({ ...settings, translate: { ...settings.translate, enabled } })}
                label="翻译"
                hint="关闭后只把语音转成文字"
              />
              {settings.translate.enabled && (
                <div className="border-t border-line pt-6">
                  <LlmForm draft={settings} onChange={onChange} presets={presets} />
                </div>
              )}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
