// 历史记录窗口：左侧列表，右侧内容 + 复制 / 导出 / 删除
import { useEffect, useState } from "react";
import { api, errorText, events, formatDateTime, type ExportFormat, type HistoryRecord, type HistorySummary } from "../../lib/ipc";
import { Button, Menu, SOURCE_LABEL, cx } from "../../components/ui";
import { IconCheck, IconCopy, IconExport, IconHistory, IconTrash } from "../../components/icons";
import { useSettings } from "../../store/settings";

function friendlyDate(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  const time = `${p(d.getHours())}:${p(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86_400_000);
  if (diff === 0) return `今天 ${time}`;
  if (diff === 1) return `昨天 ${time}`;
  if (d.getFullYear() === now.getFullYear()) return `${d.getMonth() + 1}月${d.getDate()}日 ${time}`;
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`;
}

function duration(start: number, end: number): string {
  const min = Math.round((end - start) / 60_000);
  if (min < 1) return "不到 1 分钟";
  if (min < 60) return `${min} 分钟`;
  return `${Math.floor(min / 60)} 小时 ${min % 60} 分钟`;
}

function clock(ms: number): string {
  const s = Math.floor(ms / 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}` : `${p(Math.floor(s / 60))}:${p(s % 60)}`;
}

export function HistoryWindow() {
  const [list, setList] = useState<HistorySummary[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [record, setRecord] = useState<HistoryRecord | null>(null);
  const [error, setError] = useState<string | null>(null);
  const historyOn = useSettings((s) => s.settings?.history.save ?? true);

  const reload = async () => {
    const l = await api.listHistory();
    setList(l);
    setSelected((cur) => (cur && l.some((x) => x.id === cur) ? cur : (l[0]?.id ?? null)));
  };

  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await useSettings.getState().load();
      await reload();
      const un = [await events.historyChanged(() => void reload()), await events.session((ev) => ev.state === "idle" && void reload())];
      if (cancelled) return un.forEach((f) => f());
      cleanups.push(...un);
    })();
    return () => {
      cancelled = true;
      cleanups.forEach((f) => f());
    };
  }, []);

  useEffect(() => {
    setError(null);
    if (!selected) return setRecord(null);
    api
      .getHistory(selected)
      .then(setRecord)
      .catch((e) => {
        setRecord(null);
        setError(errorText(e));
      });
  }, [selected, list]);

  if (!list) return null;

  return (
    <div className="flex h-full bg-bg">
      <aside className="flex w-72 shrink-0 flex-col border-r border-line bg-surface">
        <div className="px-4 pb-3 pt-4">
          <h1 className="text-[15px] font-semibold text-fg">历史记录</h1>
          <p className="mt-0.5 text-xs text-fg-3">{list.length ? `共 ${list.length} 次` : "还没有记录"}</p>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
          {list.map((s) => (
            <button
              key={s.id}
              onClick={() => setSelected(s.id)}
              className={cx(
                "mb-0.5 flex w-full flex-col gap-0.5 rounded-lg px-3 py-2 text-left transition-colors",
                s.id === selected ? "bg-bg shadow-sm" : "hover:bg-surface-2",
              )}
            >
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-[13px] font-medium text-fg">{friendlyDate(s.startedAt)}</span>
                <span className="shrink-0 text-[11px] text-fg-3">{duration(s.startedAt, s.endedAt)}</span>
              </div>
              <div className="text-[11px] text-fg-3">
                {SOURCE_LABEL[s.source]} · {s.count} 句
              </div>
              <div className="truncate text-xs text-fg-2">{s.preview}</div>
            </button>
          ))}
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        {record ? (
          <Detail record={record} onDeleted={() => void reload()} />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
            <IconHistory size={28} className="text-fg-3" />
            <p className="text-[14px] text-fg-2">{error ?? (list.length ? "选择左侧的一条记录" : "还没有历史记录")}</p>
            {!list.length && (
              <p className="max-w-[360px] text-xs leading-relaxed text-fg-3">
                {historyOn ? "每次从「开始」到「停止」的内容会自动保存在这里。" : "保存历史记录已关闭，可以在 设置 → 通用 中打开。"}
              </p>
            )}
          </div>
        )}
      </main>
    </div>
  );
}

function Detail({ record, onDeleted }: { record: HistoryRecord; onDeleted: () => void }) {
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [exported, setExported] = useState<string | null>(null);
  const meeting = record.entries.some((e) => e.channel === "mic") && record.entries.some((e) => e.channel === "system");

  useEffect(() => {
    setConfirmDelete(false);
    setExported(null);
  }, [record.id]);

  const copy = async () => {
    const text = record.entries
      .map((e) => {
        const who = meeting ? `${e.channel === "mic" ? "我" : "对方"}：` : "";
        return e.translation ? `${who}${e.text}\n${e.translation}` : `${who}${e.text}`;
      })
      .join("\n\n");
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const doExport = async (format: ExportFormat) => {
    const path = await api.exportHistory(record.id, format, `听译记录 ${formatDateTime(record.startedAt)}`);
    if (path) setExported(path);
  };

  const del = async () => {
    if (!confirmDelete) return setConfirmDelete(true);
    await api.deleteHistory(record.id);
    onDeleted();
  };

  return (
    <>
      <header className="flex h-16 shrink-0 items-center gap-1.5 border-b border-line px-6">
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold text-fg">{formatDateTime(record.startedAt)}</div>
          <div className="text-xs text-fg-3">
            {SOURCE_LABEL[record.source]} · {record.entries.length} 句 · {duration(record.startedAt, record.endedAt)}
          </div>
        </div>
        {exported && (
          <button className="mr-1 text-xs text-ok hover:underline" onClick={() => void api.revealFile(exported)}>
            已导出，在访达中显示
          </button>
        )}
        <Button size="sm" variant="ghost" onClick={() => void copy()}>
          {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
          {copied ? "已复制" : "复制"}
        </Button>
        <Menu
          placement="down"
          align="right"
          trigger={(toggle) => (
            <Button size="sm" variant="ghost" onClick={toggle}>
              <IconExport size={14} />
              导出
            </Button>
          )}
          items={[
            { label: "纯文本 (.txt)", hint: "原文和译文逐句排列", onSelect: () => void doExport("txt") },
            { label: "Markdown (.md)", hint: "带时间，适合做笔记", onSelect: () => void doExport("md") },
            { label: "SRT 字幕 (.srt)", hint: "带时间轴，可配合视频使用", onSelect: () => void doExport("srt") },
          ]}
        />
        <Button size="sm" variant="ghost" onClick={() => void del()} className={cx(confirmDelete && "text-danger")}>
          <IconTrash size={14} />
          {confirmDelete ? "再点一次删除" : "删除"}
        </Button>
      </header>
      <div className="selectable min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <div className="mx-auto flex max-w-[720px] flex-col gap-5">
          {record.entries.map((e) => (
            <div key={e.id} className="flex gap-3">
              <span className="mt-[3px] w-11 shrink-0 text-[11px] tabular-nums text-fg-3">{clock(e.startMs)}</span>
              {meeting && (
                <span
                  className={cx(
                    "mt-[1px] h-5 shrink-0 rounded-md px-1.5 text-[11px] font-medium leading-5",
                    e.channel === "mic" ? "bg-accent-soft text-accent" : "bg-surface-2 text-fg-2",
                  )}
                >
                  {e.channel === "mic" ? "我" : "对方"}
                </span>
              )}
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <p className={cx("break-words leading-relaxed", e.translation ? "text-[14px] text-fg-2" : "text-[15px] text-fg")}>{e.text}</p>
                {e.translation && <p className="break-words text-[16px] font-medium leading-relaxed text-fg">{e.translation}</p>}
              </div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
