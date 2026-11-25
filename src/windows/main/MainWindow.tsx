// 主窗口：未配置 → 首次启动向导；未开始且没有记录 → 引导页；否则 → 记录页
import { useEffect } from "react";
import { api } from "../../lib/ipc";
import { syncSettingsAcrossWindows, useSettings } from "../../store/settings";
import { initSessionListeners, startListening, toggleListening, useSession } from "../../store/session";
import { Onboarding } from "../onboarding/Onboarding";
import { IdleView } from "./IdleView";
import { Notices } from "./Notices";
import { TopBar } from "./TopBar";
import { BottomBar, Transcript } from "./Transcript";

export function MainWindow() {
  const settings = useSettings((s) => s.settings);
  const presets = useSettings((s) => s.presets);
  const hasItems = useSession((s) => s.items.length > 0);
  const state = useSession((s) => s.state);

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await useSettings.getState().load();
      const un = [await initSessionListeners(), await syncSettingsAcrossWindows()];
      if (cancelled) return un.forEach((f) => f());
      cleanups.push(...un);
      const s = useSettings.getState().settings;
      if (s?.onboarded && s.autoStart) void startListening();
    })();

    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey) return;
      if (e.key === ",") {
        e.preventDefault();
        void api.openSettings();
      } else if (e.key === "Enter" && useSettings.getState().settings?.onboarded) {
        e.preventDefault();
        void toggleListening();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      cancelled = true;
      cleanups.forEach((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  if (!settings || !presets) return null;
  if (!settings.onboarded) return <Onboarding />;

  const showIdle = !hasItems && state === "idle";
  return (
    <div className="flex h-full flex-col bg-bg">
      <TopBar minimal={showIdle} />
      <Notices />
      {showIdle ? <IdleView /> : <Transcript />}
      {!showIdle && <BottomBar />}
    </div>
  );
}
