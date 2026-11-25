// 顶栏下方的提示：错误横幅、缺少权限
import { api } from "../../lib/ipc";
import { Button, IconButton } from "../../components/ui";
import { IconAlert, IconLock, IconX } from "../../components/icons";
import { forceStart, startListening, useSession } from "../../store/session";

export function Notices() {
  const banner = useSession((s) => s.banner);
  const permissionIssue = useSession((s) => s.permissionIssue);
  const dismiss = useSession((s) => s.dismissBanner);

  return (
    <>
      {permissionIssue && (
        <div className="mx-4 mb-2 flex items-start gap-3 rounded-xl border border-line bg-surface px-4 py-3">
          <IconLock size={17} className="mt-0.5 shrink-0 text-accent" />
          <div className="flex flex-1 flex-col gap-2">
            <div className="text-[13px] font-medium text-fg">需要「{permissionIssue === "mic" ? "麦克风" : "屏幕与系统录音"}」权限</div>
            <div className="text-[13px] leading-relaxed text-fg-2">
              请在「系统设置 → 隐私与安全性 → {permissionIssue === "mic" ? "麦克风" : "屏幕与系统录音"}」中允许「听译」，然后点「重新开始」。
            </div>
            <div className="flex gap-2">
              <Button size="sm" variant="primary" onClick={() => void api.openPrivacySettings(permissionIssue)}>
                打开系统设置
              </Button>
              <Button size="sm" onClick={() => void startListening()}>
                重新开始
              </Button>
              <Button size="sm" variant="ghost" onClick={() => void forceStart()}>
                仍然开始
              </Button>
            </div>
          </div>
          <IconButton label="关闭" onClick={() => useSession.setState({ permissionIssue: null })}>
            <IconX size={15} />
          </IconButton>
        </div>
      )}
      {banner && (
        <div className="mx-4 mb-2 flex items-start gap-2.5 rounded-xl bg-danger-soft px-3.5 py-2.5 text-[13px] text-danger">
          <IconAlert size={16} className="mt-0.5 shrink-0" />
          <span className="selectable flex-1 break-all leading-relaxed">{banner.message}</span>
          {banner.action && (
            <Button size="sm" onClick={() => void api.openSettings(banner.action!.tab)}>
              {banner.action.label}
            </Button>
          )}
          <button className="mt-0.5 shrink-0 opacity-70 hover:opacity-100" onClick={dismiss} aria-label="关闭">
            <IconX size={15} />
          </button>
        </div>
      )}
    </>
  );
}
