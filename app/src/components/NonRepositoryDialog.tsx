import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useModalListNav } from "../lib/listNav";
import { useT } from "../lib/i18n";
import { Button } from "./ui/Button";
import { CloudIcon, FolderIcon, PlusIcon, SpinnerIcon } from "./icons";

export type RepositorySetupMode = "local" | "remote";

/**
 * 用户选中普通文件夹时的就地引导。
 * 初始化不会改动已有文件；“远程”模式只在初始化完成后打开现有远程管理器，
 * 避免把网络失败和不可回滚的 git init 伪装成一个原子操作。
 */
export function NonRepositoryDialog({
  path,
  busy,
  error,
  onContinue,
  onClone,
  onCancel,
}: {
  path: string;
  busy: boolean;
  error?: string | null;
  onContinue: (mode: RepositorySetupMode) => void;
  onClone: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [mode, setMode] = useState<RepositorySetupMode>("local");
  const modes: RepositorySetupMode[] = ["local", "remote"];
  const index = modes.indexOf(mode);
  const close = () => { if (!busy) onCancel(); };
  const { dialogRef, onKeyDown } = useModalListNav({
    count: modes.length,
    index,
    onSelect: (next) => setMode(modes[next]),
    onClose: close,
  });

  useEffect(() => {
    const appRoot = document.getElementById("root");
    if (!appRoot) return;
    const wasInert = appRoot.hasAttribute("inert");
    const previousAriaHidden = appRoot.getAttribute("aria-hidden");
    appRoot.setAttribute("inert", "");
    appRoot.setAttribute("aria-hidden", "true");
    return () => {
      if (!wasInert) appRoot.removeAttribute("inert");
      if (previousAriaHidden === null) appRoot.removeAttribute("aria-hidden");
      else appRoot.setAttribute("aria-hidden", previousAriaHidden);
    };
  }, []);

  return createPortal(
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4 sm:p-6"
      onClick={close}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="non-repository-title"
        aria-describedby="non-repository-description"
        tabIndex={-1}
        onKeyDown={onKeyDown}
        onClick={(event) => event.stopPropagation()}
        className="panel-in popover w-full max-w-[480px] overflow-hidden rounded-lg border border-line-strong bg-canvas"
      >
        <div className="flex items-start gap-3 border-b border-line px-4 py-4">
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-accent/10 text-accent">
            <FolderIcon width={16} height={16} />
          </span>
          <div className="min-w-0">
            <h2 id="non-repository-title" className="text-sm font-semibold text-fg">
              {t("nonRepo.title")}
            </h2>
            <p id="non-repository-description" className="mt-1 text-xs leading-5 text-fg-muted">
              {t("nonRepo.description")}
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          <div className="flex items-center gap-2 rounded-md bg-elevated px-2.5 py-2" title={path}>
            <FolderIcon width={13} height={13} className="shrink-0 text-fg-subtle" />
            <span className="truncate font-mono text-[11px] text-fg-muted" data-testid="non-repository-path">
              {path}
            </span>
          </div>

          <fieldset disabled={busy} className="flex flex-col gap-2">
            <legend className="mb-1 text-xs font-medium text-fg">{t("nonRepo.chooseAction")}</legend>
            <SetupChoice
              checked={mode === "local"}
              icon={<PlusIcon width={15} height={15} />}
              title={t("nonRepo.localTitle")}
              description={t("nonRepo.localDescription")}
              onChange={() => setMode("local")}
            />
            <SetupChoice
              checked={mode === "remote"}
              icon={<CloudIcon width={15} height={15} />}
              title={t("nonRepo.remoteTitle")}
              description={t("nonRepo.remoteDescription")}
              onChange={() => setMode("remote")}
            />
          </fieldset>

          {error && (
            <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-2.5 py-2 text-xs text-danger">
              {error}
            </p>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-line px-4 py-3">
          <Button type="button" variant="ghost" size="md" disabled={busy} onClick={onClone}>
            {t("nonRepo.cloneInstead")}
          </Button>
          <div className="ml-auto flex gap-2">
            <Button type="button" variant="ghost" size="md" disabled={busy} onClick={onCancel}>
              {t("confirm.cancel")}
            </Button>
            <Button type="button" variant="commit" size="md" disabled={busy} onClick={() => onContinue(mode)}>
              {busy ? (
                <span className="flex items-center gap-1.5">
                  <SpinnerIcon width={13} height={13} /> {t("nonRepo.initializing")}
                </span>
              ) : mode === "remote" ? t("nonRepo.initAndRemote") : t("nonRepo.initialize")}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}

function SetupChoice({
  checked,
  icon,
  title,
  description,
  onChange,
}: {
  checked: boolean;
  icon: ReactNode;
  title: string;
  description: string;
  onChange: () => void;
}) {
  return (
    <label className={`flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2.5 transition-colors ${checked ? "border-accent bg-accent/8" : "border-line-strong bg-elevated/40 hover:bg-elevated"}`}>
      <input
        type="radio"
        name="repository-setup-mode"
        checked={checked}
        onChange={onChange}
        className="mt-1 accent-[var(--color-accent)]"
      />
      <span className={`mt-0.5 shrink-0 ${checked ? "text-accent" : "text-fg-subtle"}`}>{icon}</span>
      <span className="min-w-0">
        <span className="block text-xs font-medium text-fg">{title}</span>
        <span className="mt-0.5 block text-[11px] leading-4 text-fg-muted">{description}</span>
      </span>
    </label>
  );
}
