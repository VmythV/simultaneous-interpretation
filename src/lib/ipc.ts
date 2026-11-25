// 与 Rust 后端通信：类型定义与命令封装（字段与 src-tauri/src 中的结构一一对应）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Source = "mic" | "system";
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

export interface Settings {
  onboarded: boolean;
  source: Source;
  micDevice: string;
  targetLang: string;
  fallbackLang: string;
  autoStart: boolean;
  silenceMs: number;
  maxSegmentSec: number;
  vadThreshold: number;
  stt: SttSettings;
  translate: LlmSettings;
}

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

export type SettingsTab = "general" | "stt" | "translate";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getPresets: () => invoke<Presets>("get_presets"),
  listInputDevices: () => invoke<InputDevice[]>("list_input_devices"),
  getSessionState: () => invoke<SessionState>("get_session_state"),
  start: () => invoke<void>("start_session"),
  stop: () => invoke<void>("stop_session"),
  checkPermission: (kind: Source) => invoke<Permission>("check_permission", { kind }),
  requestPermission: (kind: Source) => invoke<Permission>("request_permission", { kind }),
  openPrivacySettings: (kind: Source) => invoke<void>("open_privacy_settings", { kind }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  testStt: (settings: Settings) => invoke<TestResult>("test_stt", { settings }),
  testTranslate: (settings: Settings) => invoke<TestResult>("test_translate", { settings }),
  retranslate: (uttId: number, text: string) => invoke<void>("retranslate", { uttId, text }),
  openSettings: (tab?: SettingsTab) => invoke<void>("open_settings", { tab: tab ?? null }),
};

export const events = {
  pipeline: (cb: (ev: PipelineEvent) => void): Promise<UnlistenFn> => listen<PipelineEvent>("pipeline", (e) => cb(e.payload)),
  session: (cb: (ev: SessionEvent) => void): Promise<UnlistenFn> => listen<SessionEvent>("session", (e) => cb(e.payload)),
  settingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> => listen<Settings>("settings-changed", (e) => cb(e.payload)),
  settingsTab: (cb: (tab: SettingsTab) => void): Promise<UnlistenFn> => listen<SettingsTab>("settings-tab", (e) => cb(e.payload)),
};

/** invoke 失败时 reject 的是字符串 */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
