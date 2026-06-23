import { useSparseCheckout } from "../lib/queries";
import { FolderIcon } from "../components/icons";
import { EmptyHint } from "../components/ui/EmptyHint";
import type { IpcError } from "../ipc";
import { useT } from "../lib/i18n";

export function SparseCheckoutView({ repo }: { repo: string }) {
  const t = useT();
  const q = useSparseCheckout(repo);
  const patterns = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-baseline gap-3 border-b border-line px-4 py-3">
        <h1 className="serif text-[27px] font-normal leading-none text-fg">{t("title.sparse")}</h1>
        {patterns.length > 0 && <span className="font-mono text-xs text-fg-subtle">{patterns.length} {t("count.items")}</span>}
      </div>

      {q.isLoading ? (
        <Center>{t("common.loading")}</Center>
      ) : queryErr ? (
        <Center>{queryErr}</Center>
      ) : patterns.length === 0 ? (
        <EmptyHint icon={<FolderIcon width={24} height={24} />}>{t("sparse.empty")}</EmptyHint>
      ) : (
        <div className="fade-in flex-1 overflow-auto px-6 py-5">
          <div className="mx-auto max-w-[860px]">
            <p className="mb-3 max-w-[70ch] text-xs leading-relaxed text-fg-muted">{t("sparse.desc")}</p>
            <ul className="flex flex-col gap-1">
              {patterns.map((p, i) => (
                <li
                  key={i}
                  className="rounded border border-line bg-elevated px-2.5 py-1.5 font-mono text-[12px] text-fg"
                >
                  {p}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
