import { useSparseCheckout } from "../lib/queries";
import { FolderIcon } from "../components/icons";
import { EmptyHint } from "../components/ui/EmptyHint";
import { SecondaryHeader, CardTable, CardRow, Cell } from "../components/ui/CardTable";
import type { IpcError } from "../ipc";
import { useT } from "../lib/i18n";

export function SparseCheckoutView({ repo }: { repo: string }) {
  const t = useT();
  const q = useSparseCheckout(repo);
  const patterns = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  if (q.isLoading) return <Center>{t("common.loading")}</Center>;
  if (queryErr) return <Center>{queryErr}</Center>;

  return (
    <div className="fade-in h-full overflow-auto px-7 py-8">
      {/* 居中卡片表格(max 860) */}
      <div className="mx-auto max-w-[860px]">
        <SecondaryHeader
          icon={<FolderIcon width={17} height={17} />}
          title={t("title.sparse")}
          count={patterns.length > 0 ? `${patterns.length} ${t("count.items")}` : undefined}
        />
        {patterns.length === 0 ? (
          <EmptyHint icon={<FolderIcon width={24} height={24} />}>{t("sparse.empty")}</EmptyHint>
        ) : (
          <>
            <p className="mb-4 max-w-[70ch] text-xs leading-relaxed text-fg-muted">{t("sparse.desc")}</p>
            <CardTable cols={[t("col.pattern"), t("col.type")]}>
              {patterns.map((p, i) => {
                // git sparse-checkout 模式:以 ! 开头为排除,其余为包含。
                const exclude = p.startsWith("!");
                return (
                  <CardRow key={i}>
                    <Cell first>
                      <span className="truncate" title={p}>{p}</span>
                    </Cell>
                    <Cell className="!font-sans">
                      <span className={exclude ? "text-danger" : "text-success"}>
                        {exclude ? t("sparse.exclude") : t("sparse.include")}
                      </span>
                    </Cell>
                  </CardRow>
                );
              })}
            </CardTable>
          </>
        )}
      </div>
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
