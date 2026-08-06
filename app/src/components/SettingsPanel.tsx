import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
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

const PROVIDERS: SettingsSection[] = ["deepseek", "github", "gitlab"];

type CredentialStatuses = Partial<Record<SettingsSection, boolean>>;
type Operation = "save" | "test" | "clear";

export function SettingsPanel({
  onClose,
  initialSection = "deepseek",
  returnFocusRef,
}: {
  onClose: () => void;
  initialSection?: SettingsSection;
  returnFocusRef?: RefObject<HTMLElement | null>;
}) {
  const t = useT();
  const toast = useToast();
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(
    document.activeElement instanceof HTMLElement ? document.activeElement : null,
  );
  const mountedRef = useRef(true);
  const statusGenerationRef = useRef(0);
  const operationGenerationRef = useRef(0);
  const providerRef = useRef<SettingsSection>(initialSection);
  const [provider, setProvider] = useState<SettingsSection>(initialSection);
  const [statuses, setStatuses] = useState<CredentialStatuses>({});
  const [statusErrors, setStatusErrors] = useState<SettingsSection[]>([]);
  const [secret, setSecret] = useState("");
  const [loading, setLoading] = useState(true);
  const [activeOperation, setActiveOperation] = useState<Operation | null>(null);
  const busy = loading || activeOperation !== null;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      statusGenerationRef.current += 1;
      operationGenerationRef.current += 1;
    };
  }, []);

  useEffect(() => {
    const generation = ++statusGenerationRef.current;
    Promise.allSettled(PROVIDERS.map((kind) => credentialStatus(kind)))
      .then((results) => {
        if (!mountedRef.current || statusGenerationRef.current !== generation) return;
        const nextStatuses: CredentialStatuses = {};
        const nextErrors: SettingsSection[] = [];
        results.forEach((result, index) => {
          const kind = PROVIDERS[index];
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
        if (mountedRef.current && statusGenerationRef.current === generation) setLoading(false);
      });
    return () => {
      statusGenerationRef.current += 1;
    };
  }, [toast]);

  useLayoutEffect(() => {
    const previousFocus = previousFocusRef.current;
    dialogRef.current?.focus();
    return () => {
      const target = returnFocusRef?.current;
      if (target?.isConnected) target.focus();
      else if (previousFocus?.isConnected) previousFocus.focus();
    };
  }, [returnFocusRef]);

  useEffect(() => {
    if (!loading) inputRef.current?.focus();
  }, [loading]);

  useLayoutEffect(() => {
    if (activeOperation) dialogRef.current?.focus();
  }, [activeOperation]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
      if (event.key === "Tab") containFocus(event, dialogRef.current);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  function selectProvider(next: SettingsSection) {
    if (busy || next === provider) return;
    operationGenerationRef.current += 1;
    providerRef.current = next;
    setSecret("");
    setProvider(next);
  }

  async function runOperation(operation: Operation) {
    if (busy) return;
    if (operation === "save" && !secret.trim()) return;
    const operationGeneration = ++operationGenerationRef.current;
    const operationProvider = provider;
    const isCurrent = () =>
      mountedRef.current &&
      operationGenerationRef.current === operationGeneration &&
      providerRef.current === operationProvider;
    setActiveOperation(operation);
    try {
      if (operation === "save") {
        await saveCredential(operationProvider, secret);
        if (!isCurrent()) return;
        setSecret("");
        setStatuses((current) => ({ ...current, [operationProvider]: true }));
        setStatusErrors((current) => current.filter((kind) => kind !== operationProvider));
        toast({ kind: "success", title: t(providerMessageKey(operationProvider, "saved")) });
      } else if (operation === "test") {
        await testCredential(operationProvider);
        if (!isCurrent()) return;
        toast({ kind: "success", title: t(providerMessageKey(operationProvider, "valid")) });
      } else {
        await clearCredential(operationProvider);
        if (!isCurrent()) return;
        setSecret("");
        setStatuses((current) => ({ ...current, [operationProvider]: false }));
        setStatusErrors((current) => current.filter((kind) => kind !== operationProvider));
        toast({ kind: "success", title: t(providerMessageKey(operationProvider, "cleared")) });
      }
    } catch (error) {
      if (!isCurrent()) return;
      toast({ kind: "error", title: errorMessage(error) });
    } finally {
      if (isCurrent()) setActiveOperation(null);
    }
  }

  const configured = statuses[provider] === true;
  const statusKnown = statuses[provider] !== undefined;
  const statusFailed = statusErrors.includes(provider);

  function onTabKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    const currentIndex = PROVIDERS.indexOf(provider);
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % PROVIDERS.length;
    if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + PROVIDERS.length) % PROVIDERS.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = PROVIDERS.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    const next = PROVIDERS[nextIndex];
    selectProvider(next);
    document.getElementById(`settings-tab-${next}`)?.focus();
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6">
      <div
        data-testid="settings-backdrop"
        className="overlay-in absolute inset-0 bg-black/40"
        onClick={busy ? undefined : onClose}
      />
      <section
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
        aria-modal="true"
        aria-labelledby="settings-title"
        className="panel-in popover relative flex h-[min(680px,calc(100vh-48px))] w-[min(960px,calc(100vw-48px))] max-h-none max-w-none flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
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

        <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-[auto_minmax(0,1fr)] sm:grid-cols-[150px_minmax(0,1fr)] sm:grid-rows-1">
          <nav
            aria-label={t("settings.categories")}
            className="flex flex-row gap-1 border-b border-line bg-elevated p-2 sm:flex-col sm:border-b-0 sm:border-r"
          >
            <div
              aria-current="page"
              className="rounded-md bg-accent/15 px-3 py-2 text-xs font-medium text-accent"
            >
              {t("settings.integrations.title")}
            </div>
          </nav>

          <div className="flex min-h-0 min-w-0 flex-col overflow-hidden">
            <div className="border-b border-line px-3 py-3 sm:px-5 sm:py-4">
              <h3 className="text-base font-semibold text-fg">
                {t("settings.integrations.title")}
              </h3>
              <p className="mt-1 text-xs leading-relaxed text-fg-muted">
                {t("settings.integrations.description")}
              </p>
              <div
                role="tablist"
                aria-label={t("settings.integrations.providers")}
                className="mt-4 flex gap-1 overflow-x-auto border-b border-line"
              >
                {PROVIDERS.map((kind) => (
                  <button
                    key={kind}
                    type="button"
                    role="tab"
                    id={`settings-tab-${kind}`}
                    aria-controls={`settings-panel-${kind}`}
                    aria-selected={provider === kind}
                    tabIndex={provider === kind ? 0 : -1}
                    disabled={busy}
                    onClick={() => selectProvider(kind)}
                    onKeyDown={onTabKeyDown}
                    className={`shrink-0 border-b-2 px-3 py-2 text-xs font-medium transition-colors disabled:opacity-50 ${
                      provider === kind
                        ? "border-accent text-accent"
                        : "border-transparent text-fg-muted hover:text-fg"
                    }`}
                  >
                    {t(providerMessageKey(kind, "name"))}
                  </button>
                ))}
              </div>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto">
              {PROVIDERS.map((kind) => (
                <div
                  key={kind}
                  role="tabpanel"
                  id={`settings-panel-${kind}`}
                  aria-labelledby={`settings-tab-${kind}`}
                  aria-busy={provider === kind && busy}
                  hidden={provider !== kind}
                  className="min-h-0 px-3 py-4 sm:px-5 sm:py-5"
                >
                  {provider === kind && (
                <>
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h3 className="text-base font-semibold text-fg">
                        {t(providerMessageKey(provider, "name"))}
                      </h3>
                      <p className="mt-1 text-xs leading-relaxed text-fg-muted">
                        {t(providerMessageKey(provider, "description"))}
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

                  {provider === "deepseek" && (
                    <section className="mt-4">
                      <h4 className="text-xs font-semibold text-fg">
                        {t("settings.serviceDetails")}
                      </h4>
                      <dl className="mt-2 grid gap-1.5 text-xs">
                        <ServiceDetail
                          label={t("settings.deepseek.endpoint")}
                          value="https://api.deepseek.com"
                        />
                        <ServiceDetail
                          label={t("settings.deepseek.model")}
                          value="deepseek-v4-flash"
                        />
                      </dl>
                      <p id="settings-deepseek-disclosure" className="mt-3 text-xs leading-relaxed text-fg-muted">
                        {t("settings.deepseek.disclosure")}
                      </p>
                    </section>
                  )}

                  <label className="mt-5 flex flex-col gap-1.5">
                    <span className="text-xs font-medium text-fg-subtle">
                      {t(providerMessageKey(provider, "credentialLabel"))}
                    </span>
                    <input
                      ref={inputRef}
                      type="password"
                      autoComplete="new-password"
                      value={secret}
                      disabled={busy}
                      onChange={(event) => setSecret(event.target.value)}
                      placeholder={t(providerMessageKey(provider, configured ? "replacementPlaceholder" : "placeholder"))}
                      aria-describedby={`settings-${provider}-credential-helper${provider === "deepseek" ? " settings-deepseek-disclosure" : ""}`}
                      className="field h-9 rounded bg-canvas px-2.5 font-mono text-xs text-fg placeholder:text-fg-subtle disabled:opacity-50"
                    />
                  </label>
                  <p id={`settings-${provider}-credential-helper`} className="mt-1.5 text-xs text-fg-muted">
                    {t("settings.credentialHelper")}
                  </p>

                </>
                  )}
                </div>
              ))}
            </div>

            <div
              data-testid="settings-action-bar"
              className="flex shrink-0 flex-col items-stretch gap-2 border-t border-line bg-canvas px-3 py-4 min-[441px]:flex-row min-[441px]:items-center sm:px-5"
            >
              {configured && (
                <Button
                  type="button"
                  variant="danger"
                  onClick={() => void runOperation("clear")}
                  disabled={busy}
                  className="w-full min-[441px]:w-auto"
                >
                  {activeOperation === "clear" && <SpinnerIcon width={13} height={13} />}
                  {t("settings.action.removeCredential")}
                </Button>
              )}
              <div className="flex flex-col gap-2 min-[441px]:ml-auto min-[441px]:flex-row">
                {configured && (
                  <Button
                    type="button"
                    onClick={() => void runOperation("test")}
                    disabled={busy}
                    className="w-full min-[441px]:w-auto"
                  >
                    {activeOperation === "test" && <SpinnerIcon width={13} height={13} />}
                    {t("settings.action.testConnection")}
                  </Button>
                )}
                <Button
                  type="button"
                  variant="primary"
                  onClick={() => void runOperation("save")}
                  disabled={busy || !secret.trim()}
                  className="w-full min-[441px]:w-auto"
                >
                  {activeOperation === "save" && <SpinnerIcon width={13} height={13} />}
                  {t(configured ? "settings.action.saveReplacement" : "settings.action.saveCredential")}
                </Button>
              </div>
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
      <dt className="text-fg-subtle">{label}</dt>
      <dd className="select-all font-mono text-fg">{value}</dd>
    </div>
  );
}

type ProviderMessageSuffix =
  | "name"
  | "description"
  | "credentialLabel"
  | "placeholder"
  | "replacementPlaceholder"
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
      'button:not([disabled]):not([tabindex="-1"]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"]):not([disabled])',
    ),
  );
  if (focusable.length === 0) {
    event.preventDefault();
    dialog.focus();
    return;
  }
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
