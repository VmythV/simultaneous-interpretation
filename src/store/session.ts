// 会话状态、识别/翻译记录、悬浮字幕状态
import { create } from "zustand";
import {
  api,
  errorText,
  events,
  type Channel,
  type HistoryEntry,
  type PipelineEvent,
  type SessionEvent,
  type SessionState,
  type SettingsTab,
  type SingleSource,
  type Source,
  type UiState,
} from "../lib/ipc";
import { useSettings } from "./settings";

export type TrState = "none" | "pending" | "streaming" | "done" | "error";

export interface Item {
  id: number;
  channel: Channel;
  text: string;
  final: boolean;
  translation: string;
  trState: TrState;
  trError?: string;
  /** 第一次出现 / 确定的时间（导出 SRT 用） */
  startAt: number;
  endAt?: number;
}

export interface Banner {
  message: string;
  action?: { label: string; tab: SettingsTab };
}

interface SessionStore {
  state: SessionState;
  source: Source | null;
  items: Item[];
  /** 最多保留几条（字幕窗口只需要最近几句） */
  maxItems: number;
  level: Record<Channel, number>;
  speaking: Record<Channel, boolean>;
  /** 本次开始后最近一次检测到有声音的时间 */
  lastSoundAt: number;
  startedAt: number;
  banner: Banner | null;
  /** 缺少权限时显示的提示 */
  permissionIssue: SingleSource | null;
  ui: UiState;
  clear: () => void;
  dismissBanner: () => void;
}

export const useSession = create<SessionStore>(() => ({
  state: "idle",
  source: null,
  items: [],
  maxItems: Infinity,
  level: { mic: 0, system: 0 },
  speaking: { mic: false, system: false },
  lastSoundAt: 0,
  startedAt: 0,
  banner: null,
  permissionIssue: null,
  ui: { subtitleVisible: false, clickThrough: false },
  clear: () => useSession.setState({ items: [] }),
  dismissBanner: () => useSession.setState({ banner: null }),
}));

const set = useSession.setState;
const get = useSession.getState;

/** 把错误信息转成横幅，必要时附带"去设置"按钮 */
export function toBanner(message: string): Banner {
  const keyProblem = /API Key|401|403|Unauthorized|invalid_api_key|InvalidApiKey/i.test(message);
  const isTranslate = /翻译/.test(message);
  if (keyProblem || /地址|模型|Model|model/i.test(message)) {
    return { message, action: { label: "检查设置", tab: isTranslate ? "translate" : "stt" } };
  }
  return { message };
}

function updateItem(id: number, fn: (it: Item) => Item | null, create?: () => Item) {
  const items = get().items;
  // 新内容总在末尾，从后往前找
  let idx = -1;
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i].id === id) {
      idx = i;
      break;
    }
  }
  if (idx === -1) {
    if (!create) return;
    const created = fn(create());
    if (!created) return;
    let next = [...items, created].sort((a, b) => a.id - b.id);
    const max = get().maxItems;
    if (next.length > max) next = next.slice(next.length - max);
    set({ items: next });
    return;
  }
  const updated = fn(items[idx]);
  const next = items.slice();
  if (updated) next[idx] = updated;
  else next.splice(idx, 1);
  set({ items: next });
}

const blank = (id: number, channel: Channel = "mic"): Item => ({
  id,
  channel,
  text: "",
  final: false,
  translation: "",
  trState: "none",
  startAt: Date.now(),
});

function onPipeline(ev: PipelineEvent) {
  switch (ev.type) {
    case "level": {
      const s = get();
      const loud = ev.rms > 0.003; // 约 -50dB
      set({
        level: { ...s.level, [ev.channel]: ev.rms },
        speaking: { ...s.speaking, [ev.channel]: ev.speaking },
        ...(loud ? { lastSoundAt: Date.now() } : {}),
      });
      break;
    }
    case "sttPartial":
      updateItem(ev.uttId, (it) => ({ ...it, channel: ev.channel, text: ev.text }), () => blank(ev.uttId, ev.channel));
      break;
    case "sttFinal": {
      const translating = useSettings.getState().settings?.translate.enabled ?? true;
      updateItem(
        ev.uttId,
        (it) => ({
          ...it,
          channel: ev.channel,
          text: ev.text,
          final: true,
          endAt: Date.now() - ev.latencyMs,
          trState: translating && it.trState === "none" ? "pending" : it.trState,
        }),
        () => blank(ev.uttId, ev.channel),
      );
      break;
    }
    case "sttDiscard":
      updateItem(ev.uttId, () => null);
      break;
    case "trStart":
      updateItem(ev.uttId, (it) => ({ ...it, translation: "", trState: "pending", trError: undefined }));
      break;
    case "trDelta":
      updateItem(ev.uttId, (it) => ({ ...it, translation: it.translation + ev.delta, trState: "streaming" }));
      break;
    case "trDone":
      updateItem(ev.uttId, (it) => ({ ...it, translation: ev.text, trState: "done" }));
      break;
    case "trError":
      updateItem(ev.uttId, (it) => ({ ...it, trState: "error", trError: ev.message }));
      break;
    case "error":
      set({ banner: toBanner(ev.message) });
      break;
    default:
      break;
  }
}

function onSession(ev: SessionEvent) {
  set({ state: ev.state, ...(ev.source ? { source: ev.source } : {}) });
  if (ev.state === "running") set({ startedAt: Date.now(), lastSoundAt: 0 });
  if (ev.state === "idle") set({ level: { mic: 0, system: 0 }, speaking: { mic: false, system: false } });
  if (ev.error) set({ banner: toBanner(ev.error) });
}

/** 订阅识别事件、会话状态、悬浮字幕状态。controller=true 时（主窗口）还处理菜单栏/快捷键发来的开始停止请求 */
export async function initSessionListeners(controller = false) {
  const un = await Promise.all([
    events.pipeline(onPipeline),
    events.session(onSession),
    events.uiState((ui) => set({ ui })),
    ...(controller
      ? [
          events.requestToggle(() => {
            // 还没完成首次配置：把主窗口（向导）调出来
            if (!useSettings.getState().settings?.onboarded) void api.showMain();
            else void toggleListening();
          }),
          events.requestSource((s) => void changeSource(s)),
        ]
      : []),
  ]);
  const [state, ui] = await Promise.all([api.getSessionState(), api.getUiState()]);
  set({ state, ui });
  return () => un.forEach((f) => f());
}

async function permissionOk(kind: SingleSource): Promise<boolean> {
  // 权限检查失败（如辅助程序异常）不阻止开始
  let p = await api.checkPermission(kind).catch(() => "unknown" as const);
  // 电脑声音需要主动申请；麦克风在打开设备时系统会自动弹窗
  if (kind === "system" && p !== "granted" && p !== "denied") p = await api.requestPermission(kind).catch(() => "unknown" as const);
  if (p === "denied") {
    set({ state: "idle", permissionIssue: kind });
    return false;
  }
  return true;
}

/** 开始：先检查权限，再启动 */
export async function startListening() {
  await useSettings.getState().flush();
  const settings = useSettings.getState().settings;
  if (!settings || get().state !== "idle") return;
  const source = settings.source;
  set({ state: "starting", source, banner: null, permissionIssue: null });
  try {
    const kinds: SingleSource[] = source === "both" ? ["mic", "system"] : [source];
    for (const k of kinds) {
      if (!(await permissionOk(k))) return;
    }
    await api.start();
  } catch (e) {
    set({ state: "idle", banner: toBanner(errorText(e)) });
  }
}

/** 不检查权限直接开始（权限提示里的"仍然开始"） */
export async function forceStart() {
  set({ permissionIssue: null, banner: null });
  try {
    await api.start();
  } catch (e) {
    set({ state: "idle", banner: toBanner(errorText(e)) });
  }
}

export async function stopListening() {
  const s = get().state;
  if (s !== "running" && s !== "starting") return;
  await api.stop();
}

export async function toggleListening() {
  const s = get().state;
  if (s === "running") await stopListening();
  else if (s === "idle") await startListening();
}

/** 运行中切换音源：停止后用新音源重新开始 */
export async function restartIfRunning() {
  if (get().state !== "running") return;
  await api.stop();
  set({ state: "idle" });
  await startListening();
}

/** 切换音源（主界面、菜单栏）：保存为默认，运行中则重新开始 */
export async function changeSource(source: Source) {
  const { settings, saveNow } = useSettings.getState();
  if (!settings) return;
  if (settings.source !== source) await saveNow({ ...settings, source });
  await restartIfRunning();
}

export function retranslate(item: Item) {
  updateItem(item.id, (it) => ({ ...it, trState: "pending", trError: undefined }));
  api.retranslate(item.id, item.text).catch((e) => updateItem(item.id, (it) => ({ ...it, trState: "error", trError: errorText(e) })));
}

/** 记录里是否同时有"我"和"对方"（决定是否显示说话人标签） */
export function isMeeting(items: Item[]): boolean {
  return items.some((i) => i.channel === "mic") && items.some((i) => i.channel === "system");
}

export function transcriptText(items: Item[]): string {
  const meeting = isMeeting(items);
  return items
    .filter((it) => it.final && it.text)
    .map((it) => {
      const who = meeting ? `${it.channel === "mic" ? "我" : "对方"}：` : "";
      return it.translation ? `${who}${it.text}\n${it.translation}` : `${who}${it.text}`;
    })
    .join("\n\n");
}

/** 当前记录转成导出格式（时间相对第一句） */
export function toExportEntries(items: Item[]): HistoryEntry[] {
  const finals = items.filter((it) => it.final && it.text);
  const base = finals[0]?.startAt ?? Date.now();
  return finals.map((it) => ({
    id: it.id,
    channel: it.channel,
    text: it.text,
    translation: it.translation,
    startMs: Math.max(0, it.startAt - base),
    endMs: Math.max(0, (it.endAt ?? it.startAt) - base),
  }));
}
