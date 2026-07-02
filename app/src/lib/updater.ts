import type { ToastInput } from "../components/Toast";

export interface AppUpdate {
  version: string;
  body?: string;
  downloadAndInstall: () => Promise<void>;
}

export interface UpdateCheckDeps {
  check: () => Promise<AppUpdate | null>;
  relaunch: () => Promise<void>;
  toast: (t: ToastInput) => void;
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export async function checkForAppUpdate({
  check,
  relaunch,
  toast,
}: UpdateCheckDeps): Promise<void> {
  toast({ kind: "info", title: "正在检查更新…" });
  try {
    const update = await check();
    if (!update) {
      toast({ kind: "success", title: "已是最新版本" });
      return;
    }

    toast({
      kind: "info",
      title: `发现新版本 ${update.version}`,
      detail: update.body,
      duration: 0,
    });
    await update.downloadAndInstall();
    toast({ kind: "success", title: "更新已安装", detail: "即将重启应用" });
    await relaunch();
  } catch (e) {
    toast({ kind: "error", title: "检查更新失败", detail: errText(e) });
  }
}
