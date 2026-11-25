// 按窗口路由：#/settings[/tab] → 设置，#/subtitle → 悬浮字幕，#/history → 历史记录，其余 → 主窗口
import type { SettingsTab } from "./lib/ipc";
import { HistoryWindow } from "./windows/history/HistoryWindow";
import { MainWindow } from "./windows/main/MainWindow";
import { SettingsWindow } from "./windows/settings/SettingsWindow";
import { SubtitleWindow } from "./windows/subtitle/SubtitleWindow";

const TABS: SettingsTab[] = ["general", "stt", "translate", "subtitle", "hotkeys"];

export function App() {
  const hash = window.location.hash;
  if (hash.startsWith("#/settings")) {
    const tab = hash.split("/")[2] as SettingsTab | undefined;
    return <SettingsWindow initialTab={tab && TABS.includes(tab) ? tab : "general"} />;
  }
  if (hash.startsWith("#/subtitle")) return <SubtitleWindow />;
  if (hash.startsWith("#/history")) return <HistoryWindow />;
  return <MainWindow />;
}
