import { useEffect, useState, type ReactNode } from "react";
import { useLang } from "../lib/i18n";
import { windowLabels } from "../lib/locales/window";
import { CloseIcon, MaximizeIcon, MinimizeIcon, RestoreIcon } from "./icons";

export type WindowControlApi = {
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  close: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  onResized: (handler: () => void) => Promise<() => void>;
};

/** Windows-only caption controls. macOS keeps its native traffic lights. */
export function WindowControls({ windowApi }: { windowApi?: WindowControlApi }) {
  return <WindowsControls windowApi={windowApi} />;
}

function WindowsControls({ windowApi }: { windowApi?: WindowControlApi }) {
  const labels = windowLabels[useLang()];
  const [maximized, setMaximized] = useState(false);
  const [api, setApi] = useState<WindowControlApi | undefined>(windowApi);

  useEffect(() => {
    if (windowApi) {
      setApi(windowApi);
      return;
    }

    let active = true;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        if (active) setApi(getCurrentWindow());
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [windowApi]);

  useEffect(() => {
    if (!api) return;

    let active = true;
    let unlisten: (() => void) | undefined;
    const syncMaximized = async () => {
      try {
        const next = await api.isMaximized();
        if (active) setMaximized(next);
      } catch {
        // Browser previews have no Tauri IPC; the production shell does.
      }
    };

    void syncMaximized();
    void api.onResized(syncMaximized).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    }).catch(() => undefined);

    return () => {
      active = false;
      unlisten?.();
    };
  }, [api]);

  const run = (action: () => Promise<void>) => {
    void action().catch(() => undefined);
  };

  return (
    <div className="window-controls flex h-full shrink-0 items-stretch" data-testid="window-controls">
      <CaptionButton label={labels.minimize} disabled={!api} onClick={() => api && run(() => api.minimize())}>
        <MinimizeIcon width={12} height={12} />
      </CaptionButton>
      <CaptionButton label={maximized ? labels.restore : labels.maximize} disabled={!api} onClick={() => api && run(() => api.toggleMaximize())}>
        {maximized ? <RestoreIcon width={12} height={12} /> : <MaximizeIcon width={12} height={12} />}
      </CaptionButton>
      <CaptionButton label={labels.close} danger disabled={!api} onClick={() => api && run(() => api.close())}>
        <CloseIcon width={12} height={12} />
      </CaptionButton>
    </div>
  );
}

function CaptionButton({
  label,
  danger = false,
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className={`grid h-full w-[46px] place-items-center text-fg-muted transition-colors focus-visible:z-10 disabled:opacity-50 ${
        danger
          ? "hover:bg-[#c42b1c] hover:text-white active:bg-[#a92317]"
          : "hover:bg-overlay hover:text-fg active:bg-line-strong"
      }`}
    >
      {children}
    </button>
  );
}
