// 基础组件：按钮、输入框、下拉框、开关、分段选择、滑块、折叠区、测试按钮
import { useEffect, useId, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { errorText, formatHotkey, type Language, type Source, type TestResult } from "../lib/ipc";
import { IconCheck, IconChevron, IconExternal, IconEye, IconEyeOff, IconMic, IconMonitor, IconUsers, IconX } from "./icons";
import { api } from "../lib/ipc";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

export function Button({
  variant = "secondary",
  size = "md",
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: Size }) {
  const v: Record<Variant, string> = {
    primary: "bg-accent text-on-accent hover:bg-accent-hover disabled:opacity-50",
    secondary: "bg-surface-2 text-fg hover:brightness-95 dark:hover:brightness-125 disabled:opacity-50",
    ghost: "text-fg-2 hover:bg-surface-2 hover:text-fg disabled:opacity-40",
    danger: "bg-rec text-white hover:brightness-95 disabled:opacity-50",
  };
  const s: Record<Size, string> = {
    sm: "h-7 px-2.5 text-[13px] gap-1.5 rounded-md",
    md: "h-8 px-3.5 text-[13px] gap-2 rounded-lg",
    lg: "h-11 px-6 text-[15px] gap-2 rounded-xl font-medium",
  };
  return (
    <button
      className={cx("inline-flex items-center justify-center whitespace-nowrap transition-colors select-none", v[variant], s[size], className)}
      {...rest}
    >
      {children}
    </button>
  );
}

export function IconButton({ label, className, children, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      title={label}
      aria-label={label}
      className={cx("inline-flex h-8 w-8 items-center justify-center rounded-lg text-fg-2 hover:bg-surface-2 hover:text-fg transition-colors", className)}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Field({ label, hint, children, className }: { label: ReactNode; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cx("flex flex-col gap-1.5", className)}>
      <div className="text-[13px] font-medium text-fg">{label}</div>
      {children}
      {hint && <div className="text-xs leading-relaxed text-fg-3">{hint}</div>}
    </div>
  );
}

const inputCls =
  "h-9 w-full rounded-lg border border-line bg-bg px-3 text-[13px] text-fg placeholder:text-fg-3 outline-none focus:border-accent focus:ring-2 focus:ring-accent/20 transition";

export function TextInput({
  value,
  onChange,
  placeholder,
  list,
  mono,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  list?: string[];
  mono?: boolean;
}) {
  const id = useId();
  return (
    <>
      <input
        className={cx(inputCls, mono && "font-mono text-[12px]")}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        list={list?.length ? id : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {list?.length ? (
        <datalist id={id}>
          {list.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      ) : null}
    </>
  );
}

export function KeyInput({ value, onChange, placeholder = "粘贴 API Key" }: { value: string; onChange: (v: string) => void; placeholder?: string }) {
  const [show, setShow] = useState(false);
  return (
    <div className="relative">
      <input
        className={cx(inputCls, "pr-10 font-mono text-[12px]")}
        type={show ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        onChange={(e) => onChange(e.target.value.trim())}
      />
      <button
        type="button"
        className="absolute right-1 top-1/2 -translate-y-1/2 inline-flex h-7 w-7 items-center justify-center rounded-md text-fg-3 hover:text-fg"
        onClick={() => setShow(!show)}
        title={show ? "隐藏" : "显示"}
      >
        {show ? <IconEyeOff size={15} /> : <IconEye size={15} />}
      </button>
    </div>
  );
}

export function TextArea({ value, onChange, placeholder, rows = 6 }: { value: string; onChange: (v: string) => void; placeholder?: string; rows?: number }) {
  return (
    <textarea
      className={cx(inputCls, "h-auto py-2 leading-relaxed resize-y")}
      rows={rows}
      value={value}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  className,
  compact,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
  className?: string;
  compact?: boolean;
}) {
  return (
    <div className={cx("relative", className)}>
      <select
        className={cx(
          "w-full appearance-none rounded-lg border border-line bg-bg pl-3 pr-8 text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/20 transition",
          compact ? "h-8 text-[13px]" : "h-9 text-[13px]",
        )}
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <IconChevron size={14} className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 rotate-90 text-fg-3" />
    </div>
  );
}

export function LangSelect({
  value,
  onChange,
  languages,
  compact,
  className,
}: {
  value: string;
  onChange: (v: string) => void;
  languages: Language[];
  compact?: boolean;
  className?: string;
}) {
  return <Select value={value} onChange={onChange} options={languages.map((l) => ({ value: l.code, label: l.name }))} compact={compact} className={className} />;
}

export function Switch({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label?: ReactNode; hint?: ReactNode }) {
  return (
    <label className="flex items-start justify-between gap-4">
      {(label || hint) && (
        <div className="flex flex-col gap-0.5">
          {label && <span className="text-[13px] font-medium text-fg">{label}</span>}
          {hint && <span className="text-xs text-fg-3">{hint}</span>}
        </div>
      )}
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={cx("relative mt-0.5 h-[22px] w-[38px] shrink-0 rounded-full transition-colors", checked ? "bg-accent" : "bg-surface-2 border border-line")}
      >
        <span
          className={cx(
            "absolute top-1/2 h-[18px] w-[18px] -translate-y-1/2 rounded-full bg-white shadow transition-all",
            checked ? "left-[18px]" : "left-[2px]",
          )}
        />
      </button>
    </label>
  );
}

export const SOURCE_HINT: Record<Source, string> = {
  mic: "麦克风：适合自己说话、现场对话",
  system: "电脑声音：适合视频、网课、线上会议",
  both: "会议：同时听麦克风（我）和电脑声音（对方）。外放时会用系统回声消除，避免对方的声音被当成你说的",
};

export const SOURCE_LABEL: Record<Source, string> = { mic: "麦克风", system: "电脑声音", both: "会议" };

/** 听什么：麦克风 / 电脑声音 / 会议 */
export function SourcePicker({ value, onChange, size = "md" }: { value: Source; onChange: (v: Source) => void; size?: "md" | "lg" }) {
  const is = size === "lg" ? 17 : 15;
  const opts: { v: Source; label: string; icon: ReactNode }[] = [
    { v: "mic", label: "麦克风", icon: <IconMic size={is} /> },
    { v: "system", label: "电脑声音", icon: <IconMonitor size={is} /> },
    { v: "both", label: "会议", icon: <IconUsers size={is} /> },
  ];
  return (
    <div className={cx("inline-flex rounded-xl bg-surface-2 p-1", size === "lg" ? "h-11" : "h-9")}>
      {opts.map((o) => (
        <button
          key={o.v}
          type="button"
          onClick={() => onChange(o.v)}
          className={cx(
            "inline-flex items-center gap-1.5 rounded-lg transition-all",
            size === "lg" ? "px-4 text-[14px]" : "px-3 text-[13px]",
            value === o.v ? "bg-bg text-fg shadow-sm font-medium" : "text-fg-2 hover:text-fg",
          )}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Slider({
  value,
  onChange,
  min,
  max,
  step,
  format,
}: {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
}) {
  return (
    <div className="flex items-center gap-3">
      <input type="range" className="flex-1 accent-[var(--accent)]" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
      <span className="w-20 text-right text-[13px] tabular-nums text-fg-2">{format(value)}</span>
    </div>
  );
}

export function Collapsible({ title = "高级", children, defaultOpen = false }: { title?: string; children: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div>
      <button type="button" className="inline-flex items-center gap-1 text-[13px] text-fg-2 hover:text-fg" onClick={() => setOpen(!open)}>
        <IconChevron size={14} className={cx("transition-transform", open && "rotate-90")} />
        {title}
      </button>
      {open && <div className="mt-4 flex flex-col gap-5 border-l-2 border-line pl-4">{children}</div>}
    </div>
  );
}

export function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button type="button" className="inline-flex items-center gap-1 text-xs text-accent hover:underline" onClick={() => void api.openUrl(href)}>
      {children}
      <IconExternal size={12} />
    </button>
  );
}

/** 卡片式单选（服务商） */
export function ChoiceCards<T extends { id: string; name: string; description: string }>({
  items,
  value,
  onChange,
}: {
  items: T[];
  value: string;
  onChange: (item: T) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-2">
      {items.map((it) => {
        const active = it.id === value;
        return (
          <button
            key={it.id}
            type="button"
            onClick={() => onChange(it)}
            className={cx(
              "flex flex-col items-start gap-0.5 rounded-xl border px-3.5 py-2.5 text-left transition",
              active ? "border-accent bg-accent-soft ring-1 ring-accent" : "border-line hover:border-fg-3",
            )}
          >
            <span className="text-[13px] font-medium text-fg">{it.name}</span>
            <span className="text-xs text-fg-2">{it.description}</span>
          </button>
        );
      })}
    </div>
  );
}

/** 测试连接按钮 + 结果 */
export function TestButton({ run, okLabel, disabled }: { run: () => Promise<TestResult>; okLabel: string; disabled?: boolean }) {
  const [state, setState] = useState<"idle" | "testing" | "ok" | "err">("idle");
  const [result, setResult] = useState("");
  const onClick = async () => {
    setState("testing");
    try {
      const r = await run();
      setResult(`${okLabel}（${(r.ms / 1000).toFixed(1)} 秒）：${r.text}`);
      setState("ok");
    } catch (e) {
      setResult(errorText(e));
      setState("err");
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <div>
        <Button onClick={onClick} disabled={disabled || state === "testing"}>
          {state === "testing" && <Spinner />}
          {state === "testing" ? "测试中…" : "测试连接"}
        </Button>
      </div>
      {state === "ok" && (
        <div className="flex items-start gap-2 rounded-lg bg-ok-soft px-3 py-2 text-[13px] text-ok selectable">
          <IconCheck size={15} className="mt-0.5 shrink-0" />
          <span>{result}</span>
        </div>
      )}
      {state === "err" && (
        <div className="flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger selectable">
          <IconX size={15} className="mt-0.5 shrink-0" />
          <span className="break-all">{result}</span>
        </div>
      )}
    </div>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className="animate-spin" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity=".25" strokeWidth="3" fill="none" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" fill="none" strokeLinecap="round" />
    </svg>
  );
}

/** 下拉菜单（导出格式等） */
export function Menu({
  trigger,
  items,
  placement = "up",
  align = "left",
}: {
  trigger: (open: () => void) => ReactNode;
  items: { label: string; hint?: string; onSelect: () => void }[];
  placement?: "up" | "down";
  align?: "left" | "right";
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <div ref={ref} className="relative">
      {trigger(() => setOpen(!open))}
      {open && (
        <div
          className={cx(
            "absolute z-20 min-w-[180px] overflow-hidden rounded-xl border border-line bg-bg py-1 shadow-lg",
            placement === "up" ? "bottom-full mb-1.5" : "top-full mt-1.5",
            align === "left" ? "left-0" : "right-0",
          )}
        >
          {items.map((it) => (
            <button
              key={it.label}
              className="flex w-full flex-col items-start px-3.5 py-1.5 text-left hover:bg-surface-2"
              onClick={() => {
                setOpen(false);
                it.onSelect();
              }}
            >
              <span className="text-[13px] text-fg">{it.label}</span>
              {it.hint && <span className="text-[11px] text-fg-3">{it.hint}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 快捷键录制：点击后按下组合键（至少包含 ⌃/⌥/⌘ 之一）；Delete 清除，Esc 取消 */
export function HotkeyInput({ value, onChange, onRecording }: { value: string; onChange: (v: string) => void; onRecording?: (on: boolean) => void }) {
  const [recording, setRecording] = useState(false);
  const setRec = (on: boolean) => {
    setRecording(on);
    onRecording?.(on);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
    if (e.key === "Escape") return setRec(false);
    if ((e.key === "Backspace" || e.key === "Delete") && !e.ctrlKey && !e.altKey && !e.metaKey) {
      onChange("");
      return setRec(false);
    }
    if (!e.ctrlKey && !e.altKey && !e.metaKey) return;
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Cmd"].filter(Boolean);
    const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
    onChange([...mods, key].join("+"));
    setRec(false);
  };
  return (
    <button
      type="button"
      onClick={() => setRec(!recording)}
      onKeyDown={onKeyDown}
      onBlur={() => recording && setRec(false)}
      className={cx(
        "h-9 w-44 rounded-lg border px-3 text-left text-[13px] transition outline-none",
        recording ? "border-accent ring-2 ring-accent/20 text-accent" : "border-line bg-bg text-fg hover:border-fg-3",
      )}
    >
      {recording ? "请按下快捷键…" : value ? <span className="tracking-wider">{formatHotkey(value)}</span> : <span className="text-fg-3">未设置</span>}
    </button>
  );
}
