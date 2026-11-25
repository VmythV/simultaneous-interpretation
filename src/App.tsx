// 按窗口路由：#/settings[/tab] → 设置窗口，其余 → 主窗口
import type { SettingsTab } from "./lib/ipc";
import { MainWindow } from "./windows/main/MainWindow";
import { SettingsWindow } from "./windows/settings/SettingsWindow";

export function App() {
  const hash = window.location.hash;
  if (hash.startsWith("#/settings")) {
    const tab = hash.split("/")[2] as SettingsTab | undefined;
    return <SettingsWindow initialTab={tab && ["general", "stt", "translate"].includes(tab) ? tab : "general"} />;
  }
  return <MainWindow />;
}
