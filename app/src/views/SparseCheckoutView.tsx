import { useSparseCheckout } from "../lib/queries";
import { FolderIcon } from "../components/icons";
import { EmptyHint } from "../components/ui/EmptyHint";
import type { IpcError } from "../ipc";

export function SparseCheckoutView({ repo }: { repo: string }) {
  const q = useSparseCheckout(repo);
  const patterns = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs text-fg-muted">
        <FolderIcon width={13} height={13} />
        <span>稀疏检出范围</span>
        {patterns.length > 0 && <span className="text-fg-subtle">· {patterns.length} 条</span>}
      </div>

      {q.isLoading ? (
        <Center>加载中…</Center>
      ) : queryErr ? (
        <Center>{queryErr}</Center>
      ) : patterns.length === 0 ? (
        <EmptyHint icon={<FolderIcon width={24} height={24} />}>未开启稀疏检出</EmptyHint>
      ) : (
        <div className="fade-in flex-1 overflow-auto p-3">
          <p className="mb-2 text-xs text-fg-muted">
            稀疏检出已开启 —— 工作区只检出以下范围内的文件，其余路径不在磁盘上(更改/历史里看不到它们的工作区改动是正常的)。
          </p>
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
