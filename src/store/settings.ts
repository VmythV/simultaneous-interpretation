// 设置：加载、修改（自动保存）、跨窗口同步
import { create } from "zustand";
import { api, errorText, events, type Presets, type Settings } from "../lib/ipc";

interface SettingsStore {
  settings: Settings | null;
  presets: Presets | null;
  saving: boolean;
  savedAt: number | null;
  saveError: string | null;
  load: () => Promise<void>;
  /** 修改设置并在 400ms 后自动保存 */
  update: (fn: (s: Settings) => Settings) => void;
  /** 立即保存（如向导完成时） */
  saveNow: (s: Settings) => Promise<void>;
  /** 有未保存的修改时立即保存（开始识别前调用，保证后端用的是最新设置） */
  flush: () => Promise<void>;
}

let timer: ReturnType<typeof setTimeout> | undefined;
let lastLocalEdit = 0;

export const useSettings = create<SettingsStore>((set, get) => ({
  settings: null,
  presets: null,
  saving: false,
  savedAt: null,
  saveError: null,

  load: async () => {
    const [settings, presets] = await Promise.all([api.getSettings(), api.getPresets()]);
    set({ settings, presets });
  },

  update: (fn) => {
    const cur = get().settings;
    if (!cur) return;
    const next = fn(cur);
    lastLocalEdit = Date.now();
    set({ settings: next });
    clearTimeout(timer);
    timer = setTimeout(() => void get().saveNow(get().settings!), 400);
  },

  flush: async () => {
    const s = get().settings;
    if (timer !== undefined && s) await get().saveNow(s);
  },

  saveNow: async (s) => {
    clearTimeout(timer);
    timer = undefined;
    lastLocalEdit = Date.now();
    set({ settings: s, saving: true });
    try {
      await api.saveSettings(s);
      set({ saving: false, savedAt: Date.now(), saveError: null });
    } catch (e) {
      set({ saving: false, saveError: errorText(e) });
    }
  },
}));

/** 订阅其他窗口保存的设置。本窗口刚编辑过（1.5 秒内）时忽略回声，避免覆盖正在输入的内容。 */
export function syncSettingsAcrossWindows() {
  return events.settingsChanged((s) => {
    if (Date.now() - lastLocalEdit < 1500) return;
    useSettings.setState({ settings: s });
  });
}

/** 修改目标语言；备用语言不能和目标语言相同 */
export function withTarget(s: Settings, targetLang: string): Settings {
  const fallbackLang = s.fallbackLang === targetLang ? (targetLang === "zh" ? "en" : "zh") : s.fallbackLang;
  return { ...s, targetLang, fallbackLang };
}
