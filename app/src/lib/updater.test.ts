import { describe, expect, it, vi } from "vitest";
import { checkForAppUpdate, type UpdateCheckDeps } from "./updater";

function deps(update: Awaited<ReturnType<UpdateCheckDeps["check"]>>): {
  deps: UpdateCheckDeps;
  toasts: Parameters<UpdateCheckDeps["toast"]>[0][];
  relaunch: ReturnType<typeof vi.fn<() => Promise<void>>>;
} {
  const toasts: Parameters<UpdateCheckDeps["toast"]>[0][] = [];
  const relaunch = vi.fn<() => Promise<void>>().mockResolvedValue(undefined);
  return {
    toasts,
    relaunch,
    deps: {
      check: vi.fn().mockResolvedValue(update),
      relaunch,
      toast: (t) => toasts.push(t),
    },
  };
}

describe("checkForAppUpdate", () => {
  it("shows an up-to-date toast when no update exists", async () => {
    const ctx = deps(null);

    await checkForAppUpdate(ctx.deps);

    expect(ctx.toasts).toEqual([
      { kind: "info", title: "正在检查更新…" },
      { kind: "success", title: "已是最新版本" },
    ]);
    expect(ctx.relaunch).not.toHaveBeenCalled();
  });

  it("downloads, installs, and relaunches when an update exists", async () => {
    const downloadAndInstall = vi
      .fn<() => Promise<void>>()
      .mockResolvedValue(undefined);
    const ctx = deps({
      version: "0.2.0",
      body: "修复若干问题",
      downloadAndInstall,
    });

    await checkForAppUpdate(ctx.deps);

    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(ctx.relaunch).toHaveBeenCalledOnce();
    expect(ctx.toasts).toEqual([
      { kind: "info", title: "正在检查更新…" },
      {
        kind: "info",
        title: "发现新版本 0.2.0",
        detail: "修复若干问题",
        duration: 0,
      },
      { kind: "success", title: "更新已安装", detail: "即将重启应用" },
    ]);
  });

  it("reports updater errors", async () => {
    const toasts: Parameters<UpdateCheckDeps["toast"]>[0][] = [];
    const deps: UpdateCheckDeps = {
      check: vi.fn().mockRejectedValue(new Error("network failed")),
      relaunch: vi.fn(),
      toast: (t) => toasts.push(t),
    };

    await checkForAppUpdate(deps);

    expect(toasts).toEqual([
      { kind: "info", title: "正在检查更新…" },
      { kind: "error", title: "检查更新失败", detail: "network failed" },
    ]);
  });
});
