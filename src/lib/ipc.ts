// 与 Rust 后端通信：类型定义与命令封装（字段与 src-tauri/src 中的结构一一对应）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Source = "mic" | "system" | "both";
/** 单个音源（权限、通道） */
export type SingleSource = "mic" | "system";
export type Channel = "mic" | "system";
export type SttMode = "aliyun-realtime" | "openai-compatible";

export interface SttSettings {
  preset: string;
  mode: SttMode;
  baseUrl: string;
  model: string;
  apiKey: string;
  language: string;
}

export interface LlmSettings {
  enabled: boolean;
  preset: string;
  useSttKey: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
  temperature: number;
  contextSize: number;
  prompt: string;
}

export type SubtitleShow = "both" | "translation" | "original";

export interface SubtitleSettings {
  fontSize: number;
  opacity: number;
  show: SubtitleShow;
  lines: number;
  alwaysOnTop: boolean;
}

export interface HotkeySettings {
  toggle: string;
  subtitle: string;
  clickThrough: string;
}

export interface Settings {
  onboarded: boolean;
  source: Source;
  micDevice: string;
  /** 电脑声音只听这个 App（bundle ID），空字符串为全部 */
  systemApp: string;
  systemAppName: string;
  /** 会议模式下用系统回声消除采集麦克风 */
  echoCancel: boolean;
  targetLang: string;
  fallbackLang: string;
  autoStart: boolean;
  silenceMs: number;
  maxSegmentSec: number;
  vadThreshold: number;
  stt: SttSettings;
  translate: LlmSettings;
  subtitle: SubtitleSettings;
  hotkeys: HotkeySettings;
  history: { save: boolean };
}

export interface UiState {
  subtitleVisible: boolean;
  clickThrough: boolean;
}

export interface SaveResult {
  warnings: string[];
}

export interface HistoryEntry {
  id: number;
  channel: Channel;
  text: string;
  translation: string;
  startMs: number;
  endMs: number;
}

export interface HistorySummary {
  id: string;
  startedAt: number;
  endedAt: number;
  source: Source;
  count: number;
  preview: string;
}

export interface HistoryRecord {
  id: string;
  startedAt: number;
  endedAt: number;
  source: Source;
  targetLang: string;
  entries: HistoryEntry[];
}

export type ExportFormat = "txt" | "md" | "srt";

export interface SttPreset {
  id: string;
  name: string;
  description: string;
  mode: SttMode;
  baseUrl: string;
  model: string;
  models: string[];
  keyUrl: string;
  note: string;
}

export interface LlmPreset {
  id: string;
  name: string;
  description: string;
  baseUrl: string;
  model: string;
  models: string[];
  keyUrl: string;
  note: string;
}

export interface Language {
  code: string;
  name: string;
}

export interface Presets {
  stt: SttPreset[];
  llm: LlmPreset[];
  languages: Language[];
  defaultPrompt: string;
}

export interface AudioApp {
  bundleId: string;
  name: string;
  playing: boolean;
  /** PNG base64 */
  icon: string;
}

export interface InputDevice {
  id: string;
  name: string;
  is_default: boolean;
}

export type SessionState = "idle" | "starting" | "running" | "stopping";

export interface SessionEvent {
  state: SessionState;
  source: Source | null;
  error: string | null;
}

export type PipelineEvent =
  | { type: "info"; channel: Channel | null; message: string }
  | { type: "error"; channel: Channel | null; message: string }
  | { type: "level"; channel: Channel; rms: number; speaking: boolean }
  | { type: "speechStart"; channel: Channel }
  | { type: "speechEnd"; channel: Channel; speechMs: number }
  | { type: "sttPartial"; channel: Channel; uttId: number; text: string }
  | { type: "sttFinal"; channel: Channel; uttId: number; text: string; lang: string | null; latencyMs: number }
  | { type: "sttDiscard"; channel: Channel; uttId: number }
  | { type: "trStart"; uttId: number; targetLang: string }
  | { type: "trDelta"; uttId: number; delta: string }
  | { type: "trDone"; uttId: number; text: string; ttftMs: number; speechToFirstMs: number; speechToDoneMs: number }
  | { type: "trError"; uttId: number; message: string };

export type Permission = "granted" | "denied" | "undetermined" | "unknown";

export interface TestResult {
  text: string;
  ms: number;
}

export type SettingsTab = "general" | "stt" | "translate" | "subtitle" | "hotkeys";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<SaveResult>("save_settings", { settings }),
  getHotkeyErrors: () => invoke<string[]>("get_hotkey_errors"),
  suspendHotkeys: (suspend: boolean) => invoke<string[]>("suspend_hotkeys", { suspend }),
  getPresets: () => invoke<Presets>("get_presets"),
  listInputDevices: () => invoke<InputDevice[]>("list_input_devices"),
  listAudioApps: () => invoke<AudioApp[]>("list_audio_apps"),
  getSessionState: () => invoke<SessionState>("get_session_state"),
  start: () => invoke<void>("start_session"),
  stop: () => invoke<void>("stop_session"),
  checkPermission: (kind: SingleSource) => invoke<Permission>("check_permission", { kind }),
  requestPermission: (kind: SingleSource) => invoke<Permission>("request_permission", { kind }),
  openPrivacySettings: (kind: SingleSource) => invoke<void>("open_privacy_settings", { kind }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  testStt: (settings: Settings) => invoke<TestResult>("test_stt", { settings }),
  testTranslate: (settings: Settings) => invoke<TestResult>("test_translate", { settings }),
  retranslate: (uttId: number, text: string) => invoke<void>("retranslate", { uttId, text }),
  openSettings: (tab?: SettingsTab) => invoke<void>("open_settings", { tab: tab ?? null }),
  getUiState: () => invoke<UiState>("get_ui_state"),
  setSubtitleVisible: (visible: boolean) => invoke<void>("set_subtitle_visible", { visible }),
  setClickThrough: (on: boolean) => invoke<void>("set_click_through", { on }),
  showMain: () => invoke<void>("show_main"),
  openHistory: () => invoke<void>("open_history"),
  listHistory: () => invoke<HistorySummary[]>("list_history"),
  getHistory: (id: string) => invoke<HistoryRecord>("get_history", { id }),
  deleteHistory: (id: string) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  exportHistory: (id: string, format: ExportFormat, title: string) => invoke<string | null>("export_history", { id, format, title }),
  exportEntries: (entries: HistoryEntry[], format: ExportFormat, title: string) =>
    invoke<string | null>("export_entries", { entries, format, title }),
  revealFile: (path: string) => invoke<void>("reveal_file", { path }),
};

export const events = {
  pipeline: (cb: (ev: PipelineEvent) => void): Promise<UnlistenFn> => listen<PipelineEvent>("pipeline", (e) => cb(e.payload)),
  session: (cb: (ev: SessionEvent) => void): Promise<UnlistenFn> => listen<SessionEvent>("session", (e) => cb(e.payload)),
  settingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> => listen<Settings>("settings-changed", (e) => cb(e.payload)),
  settingsTab: (cb: (tab: SettingsTab) => void): Promise<UnlistenFn> => listen<SettingsTab>("settings-tab", (e) => cb(e.payload)),
  uiState: (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("ui-state", (e) => cb(e.payload)),
  requestToggle: (cb: () => void): Promise<UnlistenFn> => listen("request-toggle", () => cb()),
  requestSource: (cb: (s: Source) => void): Promise<UnlistenFn> => listen<Source>("request-source", (e) => cb(e.payload)),
  historyChanged: (cb: () => void): Promise<UnlistenFn> => listen("history-changed", () => cb()),
};

/** 本地时间格式化：2026-11-26 10:30 */
export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 快捷键显示：Ctrl+Alt+S → ⌃⌥S */
export function formatHotkey(accel: string): string {
  if (!accel) return "";
  const sym: Record<string, string> = { CTRL: "⌃", CONTROL: "⌃", ALT: "⌥", OPTION: "⌥", SHIFT: "⇧", CMD: "⌘", COMMAND: "⌘", SUPER: "⌘" };
  return accel
    .split("+")
    .map((t) => sym[t.toUpperCase()] ?? t.replace(/^Key/, "").replace(/^Digit/, ""))
    .join("");
}

/** invoke 失败时 reject 的是字符串 */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
