// 记录区：原文（灰、小）+ 译文（深、大），自动滚到最新
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../../lib/ipc";
import { Button, cx } from "../../components/ui";
import { IconArrowDown, IconCheck, IconCopy, IconTrash } from "../../components/icons";
import { useSettings } from "../../store/settings";
import { retranslate, transcriptText, useSession, type Item } from "../../store/session";

export function Transcript() {
  const items = useSession((s) => s.items);
  const translating = useSettings((s) => s.settings?.translate.enabled ?? true);
  const ref = useRef<HTMLDivElement>(null);
  const [atBottom, setAtBottom] = useState(true);

  useLayoutEffect(() => {
    const el = ref.current;
    if (el && atBottom) el.scrollTop = el.scrollHeight;
  }, [items, atBottom]);

  const onScroll = () => {
    const el = ref.current;
    if (el) setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 48);
  };

  return (
    <div className="relative min-h-0 flex-1">
      <div ref={ref} onScroll={onScroll} className="h-full overflow-y-auto px-8 pt-3 pb-8">
        {items.length === 0 ? (
          <Listening />
        ) : (
          <div className="selectable mx-auto flex max-w-[760px] flex-col gap-6">
            {items.map((it) => (
              <Row key={it.id} item={it} translating={translating} />
            ))}
          </div>
        )}
      </div>
      {!atBottom && (
        <button
          className="absolute bottom-4 left-1/2 inline-flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-line bg-bg px-3.5 py-1.5 text-[12px] text-fg-2 shadow-md hover:text-fg"
          onClick={() => setAtBottom(true)}
        >
          <IconArrowDown size={13} />
          回到最新
        </button>
      )}
    </div>
  );
}

function Row({ item, translating }: { item: Item; translating: boolean }) {
  return (
    <div className="flex flex-col gap-1.5">
      <p
        className={cx(
          "leading-relaxed break-words",
          translating ? "text-[14px]" : "text-[16px]",
          !item.final ? "caret text-fg-3" : translating ? "text-fg-2" : "text-fg",
        )}
      >
        {item.text}
      </p>
      {translating && item.final && <Translation item={item} />}
    </div>
  );
}

function Translation({ item }: { item: Item }) {
  if (item.trState === "error")
    return (
      <p className="text-[13px] text-danger">
        翻译失败
        <button className="ml-2 underline underline-offset-2" onClick={() => retranslate(item)}>
          重试
        </button>
        {item.trError && <span className="ml-2 text-fg-3">{item.trError}</span>}
      </p>
    );
  if (!item.translation)
    return (
      <p className="dots text-[18px] leading-none text-fg-3">
        <span>·</span>
        <span>·</span>
        <span>·</span>
      </p>
    );
  return <p className="text-[17px] font-medium leading-relaxed break-words text-fg">{item.translation}</p>;
}

/** 已开始但还没有内容 */
function Listening() {
  const state = useSession((s) => s.state);
  const source = useSession((s) => s.source);
  const lastSoundAt = useSession((s) => s.lastSoundAt);
  const startedAt = useSession((s) => s.startedAt);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  if (state !== "running")
    return <div className="flex h-full items-center justify-center text-[14px] text-fg-3">{state === "starting" ? "准备中…" : "已停止"}</div>;

  const noSound = lastSoundAt === 0 && startedAt > 0 && now - startedAt > 8000;
  const isSystem = source === "system";
  return (
    <div className="flex h-full flex-col items-center justify-center text-center">
      <p className="text-[15px] text-fg-2">{isSystem ? "正在听电脑声音，播放视频或会议即可" : "正在听麦克风，请开始说话"}</p>
      <p className="mt-2 text-xs text-fg-3">说的话会实时显示在这里</p>
      {noSound && (
        <div className="mt-6 max-w-[420px] rounded-xl bg-surface px-4 py-3 text-left text-[13px] leading-relaxed text-fg-2">
          还没有检测到声音。
          {isSystem
            ? "请确认电脑正在播放声音，并在「系统设置 → 隐私与安全性 → 屏幕与系统录音」中允许「听译」。"
            : "请确认麦克风没有被静音，并在「系统设置 → 隐私与安全性 → 麦克风」中允许「听译」。"}
          <div className="mt-2">
            <Button size="sm" onClick={() => void api.openPrivacySettings(isSystem ? "system" : "mic")}>
              打开系统设置
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

export function BottomBar() {
  const items = useSession((s) => s.items);
  const clear = useSession((s) => s.clear);
  const [copied, setCopied] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const count = items.filter((i) => i.final).length;

  useEffect(() => {
    if (!confirmClear) return;
    const t = setTimeout(() => setConfirmClear(false), 3000);
    return () => clearTimeout(t);
  }, [confirmClear]);

  const copy = async () => {
    await navigator.clipboard.writeText(transcriptText(items));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const onClear = () => {
    if (confirmClear) {
      clear();
      setConfirmClear(false);
    } else setConfirmClear(true);
  };

  return (
    <footer className="flex h-11 shrink-0 items-center gap-1 border-t border-line px-3">
      <Button variant="ghost" size="sm" disabled={!count} onClick={() => void copy()}>
        {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
        {copied ? "已复制" : "复制全部"}
      </Button>
      <Button variant="ghost" size="sm" disabled={!items.length} onClick={onClear} className={cx(confirmClear && "text-danger")}>
        <IconTrash size={14} />
        {confirmClear ? "再点一次清空" : "清空"}
      </Button>
      <div className="flex-1" />
      {count > 0 && <span className="pr-1 text-xs text-fg-3">{count} 句</span>}
    </footer>
  );
}
