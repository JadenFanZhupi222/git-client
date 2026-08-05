import { useEffect, useRef, useState } from "react";
import type { CredentialKindDto, IpcError } from "../bindings";
import {
  clearCredential,
  credentialStatus,
  saveCredential,
  testCredential,
} from "../ipc";
import { useT } from "../lib/i18n";
import type { SettingsSection } from "../lib/settings";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";

const SECTIONS: SettingsSection[] = ["deepseek", "github", "gitlab"];

type CredentialStatuses = Partial<Record<SettingsSection, boolean>>;
type Operation = "save" | "test" | "clear";

export function SettingsPanel({
  onClose,
  initialSection = "deepseek",
}: {
  onClose: () => void;
  initialSection?: SettingsSection;
}) {
  const t = useT();
  const toast = useToast();
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(
    document.activeElement instanceof HTMLElement ? document.activeElement : null,
  );
  const [section, setSection] = useState<SettingsSection>(initialSection);
  const [statuses, setStatuses] = useState<CredentialStatuses>({});
  const [statusErrors, setStatusErrors] = useState<SettingsSection[]>([]);
  const [secret, setSecret] = useState("");
  const [loading, setLoading] = useState(true);
  const [activeOperation, setActiveOperation] = useState<Operation | null>(null);
  const busy = loading || activeOperation !== null;

  useEffect(() => {
    let alive = true;
    Promise.allSettled(SECTIONS.map((kind) => credentialStatus(kind)))
      .then((results) => {
        if (!alive) return;
        const nextStatuses: CredentialStatuses = {};
        const nextErrors: SettingsSection[] = [];
        results.forEach((result, index) => {
          const kind = SECTIONS[index];
          if (result.status === "fulfilled") {
            nextStatuses[kind] = result.value;
          } else {
            nextErrors.push(kind);
            toast({ kind: "error", title: errorMessage(result.reason) });
          }
        });
        setStatuses(nextStatuses);
        setStatusErrors(nextErrors);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [toast]);

  useEffect(() => {
    const previousFocus = previousFocusRef.current;
    return () => previousFocus?.focus();
  }, []);

  useEffect(() => {
    if (!loading) inputRef.current?.focus();
  }, [loading]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
      if (event.key === "Tab") containFocus(event, dialogRef.current);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  function selectSection(next: SettingsSection) {
    if (busy || next === section) return;
    setSecret("");
    setSection(next);
  }

  async function runOperation(operation: Operation) {
    if (busy) return;
    if (operation === "save" && !secret.trim()) return;
    setActiveOperation(operation);
    try {
      if (operation === "save") {
        await saveCredential(section, secret);
        setSecret("");
        setStatuses((current) => ({ ...current, [section]: true }));
        toast({ kind: "success", title: t(providerMessageKey(section, "saved")) });
      } else if (operation === "test") {
        await testCredential(section);
        toast({ kind: "success", title: t(providerMessageKey(section, "valid")) });
      } else {
        await clearCredential(section);
        setSecret("");
        setStatuses((current) => ({ ...current, [section]: false }));
        toast({ kind: "success", title: t(providerMessageKey(section, "cleared")) });
      }
    } catch (error) {
      toast({ kind: "error", title: errorMessage(error) });
    } finally {
      setActiveOperation(null);
    }
  }

  const configured = statuses[section] === true;
  const statusKnown = statuses[section] !== undefined;
  const statusFailed = statusErrors.includes(section);

  function onTabKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    const currentIndex = SECTIONS.indexOf(section);
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % SECTIONS.length;
    if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + SECTIONS.length) % SECTIONS.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = SECTIONS.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    const next = SECTIONS[nextIndex];
    selectSection(next);
    document.getElementById(`settings-tab-${next}`)?.focus();
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <div
        data-testid="settings-backdrop"
        className="overlay-in absolute inset-0 bg-black/40"
        onClick={busy ? undefined : onClose}
      />
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        className="panel-in popover relative flex max-h-[82vh] w-[680px] max-w-[94vw] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
      >
        <header className="flex items-center gap-2 border-b border-line px-4 py-3">
          <h2 id="settings-title" className="text-sm font-semibold text-fg">
            {t("settings.title")}
          </h2>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onClose}
            disabled={busy}
            aria-label={t("settings.close")}
            className="ml-auto h-7 w-7 p-0"
          >
            <CloseIcon width={13} height={13} />
          </Button>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-[150px_minmax(0,1fr)]">
          <nav
            role="tablist"
            aria-label={t("settings.providers")}
            className="flex flex-col gap-1 border-r border-line bg-elevated p-2"
          >
            {SECTIONS.map((kind) => (
              <button
                key={kind}
                type="button"
                role="tab"
                id={`settings-tab-${kind}`}
                aria-controls={`settings-panel-${kind}`}
                aria-selected={section === kind}
                tabIndex={section === kind ? 0 : -1}
                disabled={busy}
                onClick={() => selectSection(kind)}
                onKeyDown={onTabKeyDown}
                className={`rounded-md px-3 py-2 text-left text-xs font-medium transition-colors disabled:opacity-50 ${
                  section === kind
                    ? "bg-accent/15 text-accent"
                    : "text-fg-muted hover:bg-overlay hover:text-fg"
                }`}
              >
                {t(providerMessageKey(kind, "name"))}
              </button>
            ))}
          </nav>

          <div
            role="tabpanel"
            id={`settings-panel-${section}`}
            aria-labelledby={`settings-tab-${section}`}
            className="min-h-0 overflow-y-auto px-5 py-5"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="text-base font-semibold text-fg">
                  {t(providerMessageKey(section, "name"))}
                </h3>
                <p className="mt-1 text-xs leading-relaxed text-fg-muted">
                  {t(providerMessageKey(section, "description"))}
                </p>
              </div>
              <span
                aria-live="polite"
                className={`shrink-0 rounded-full px-2 py-1 text-[11px] font-medium ${
                  configured ? "bg-success/15 text-success" : "bg-overlay text-fg-muted"
                }`}
              >
                {statusFailed
                  ? t("settings.status.unavailable")
                  : statusKnown
                  ? configured
                    ? t("settings.status.configured")
                    : t("settings.status.notConfigured")
                  : t("settings.status.loading")}
              </span>
            </div>

            {section === "deepseek" && (
              <div className="mt-4 grid gap-2 rounded-md border border-line bg-elevated p-3 text-xs">
                <ServiceDetail label={t("settings.deepseek.endpoint")} value="https://api.deepseek.com" />
                <ServiceDetail label={t("settings.deepseek.model")} value="deepseek-v4-flash" />
                <p className="mt-1 border-t border-line pt-2 leading-relaxed text-fg-muted">
                  {t("settings.deepseek.disclosure")}
                </p>
              </div>
            )}

            <label className="mt-5 flex flex-col gap-1.5">
              <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                {t(providerMessageKey(section, "credentialLabel"))}
              </span>
              <input
                ref={inputRef}
                type="password"
                autoComplete="new-password"
                value={secret}
                disabled={busy}
                onChange={(event) => setSecret(event.target.value)}
                placeholder={t(providerMessageKey(section, "placeholder"))}
                className="field rounded bg-canvas px-2.5 py-2 font-mono text-xs text-fg placeholder:text-fg-subtle disabled:opacity-50"
              />
            </label>

            <div className="mt-5 flex flex-wrap items-center gap-2 border-t border-line pt-4">
              <Button
                type="button"
                variant="danger"
                onClick={() => void runOperation("clear")}
                disabled={busy || !configured}
              >
                {activeOperation === "clear" && <SpinnerIcon width={13} height={13} />}
                {t("settings.action.clear")}
              </Button>
              <Button
                type="button"
                onClick={() => void runOperation("test")}
                disabled={busy || !configured}
                className="ml-auto"
              >
                {activeOperation === "test" && <SpinnerIcon width={13} height={13} />}
                {t("settings.action.test")}
              </Button>
              <Button
                type="button"
                variant="primary"
                onClick={() => void runOperation("save")}
                disabled={busy || !secret.trim()}
              >
                {activeOperation === "save" && <SpinnerIcon width={13} height={13} />}
                {t("settings.action.save")}
              </Button>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

function ServiceDetail({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[80px_minmax(0,1fr)] gap-3">
      <span className="text-fg-subtle">{label}</span>
      <span className="select-all font-mono text-fg">{value}</span>
    </div>
  );
}

type ProviderMessageSuffix =
  | "name"
  | "description"
  | "credentialLabel"
  | "placeholder"
  | "saved"
  | "valid"
  | "cleared";

function providerMessageKey(
  section: CredentialKindDto,
  suffix: ProviderMessageSuffix,
):
  | `settings.deepseek.${ProviderMessageSuffix}`
  | `settings.github.${ProviderMessageSuffix}`
  | `settings.gitlab.${ProviderMessageSuffix}` {
  return `settings.${section}.${suffix}`;
}

function errorMessage(error: unknown): string {
  return (error as IpcError)?.message ?? String(error);
}

function containFocus(event: KeyboardEvent, dialog: HTMLElement | null) {
  if (!dialog) return;
  const focusable = Array.from(
    dialog.querySelectorAll<HTMLElement>(
      'button:not([disabled]):not([tabindex="-1"]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
    ),
  );
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
