// 会话状态与识别/翻译记录
import { create } from "zustand";
import { api, errorText, events, type Channel, type PipelineEvent, type SessionEvent, type SessionState, type SettingsTab, type Source } from "../lib/ipc";
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
}

export interface Banner {
  message: string;
  action?: { label: string; tab: SettingsTab };
}

interface SessionStore {
  state: SessionState;
  source: Source | null;
  items: Item[];
  level: Record<Channel, number>;
  speaking: Record<Channel, boolean>;
  /** 本次开始后最近一次检测到有声音的时间 */
  lastSoundAt: number;
  startedAt: number;
  banner: Banner | null;
  /** 缺少权限时显示的提示 */
  permissionIssue: Source | null;
  clear: () => void;
  dismissBanner: () => void;
}

export const useSession = create<SessionStore>(() => ({
  state: "idle",
  source: null,
  items: [],
  level: { mic: 0, system: 0 },
  speaking: { mic: false, system: false },
  lastSoundAt: 0,
  startedAt: 0,
  banner: null,
  permissionIssue: null,
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
    const next = [...items, created].sort((a, b) => a.id - b.id);
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

export async function initSessionListeners() {
  const un = await Promise.all([events.pipeline(onPipeline), events.session(onSession)]);
  const state = await api.getSessionState();
  set({ state });
  return () => un.forEach((f) => f());
}

/** 开始：先检查权限，再启动 */
export async function startListening() {
  await useSettings.getState().flush();
  const settings = useSettings.getState().settings;
  if (!settings || get().state !== "idle") return;
  const kind = settings.source;
  set({ state: "starting", source: kind, banner: null, permissionIssue: null });
  try {
    // 权限检查失败（如辅助程序异常）不阻止开始
    let p = await api.checkPermission(kind).catch(() => "unknown" as const);
    // 电脑声音需要主动申请；麦克风在打开设备时系统会自动弹窗
    if (kind === "system" && p !== "granted" && p !== "denied") p = await api.requestPermission(kind).catch(() => "unknown" as const);
    if (p === "denied") {
      set({ state: "idle", permissionIssue: kind });
      return;
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

export function retranslate(item: Item) {
  updateItem(item.id, (it) => ({ ...it, trState: "pending", trError: undefined }));
  api.retranslate(item.id, item.text).catch((e) => updateItem(item.id, (it) => ({ ...it, trState: "error", trError: errorText(e) })));
}

export function transcriptText(items: Item[]): string {
  return items
    .filter((it) => it.final && it.text)
    .map((it) => (it.translation ? `${it.text}\n${it.translation}` : it.text))
    .join("\n\n");
}
