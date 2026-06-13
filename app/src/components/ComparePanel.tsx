import { useEffect, useState } from "react";
import { useCompareFiles, useCompareDiff } from "../lib/queries";
import { CommitFileList } from "./CommitFileList";
import { DiffView } from "./DiffView";
import { Resizer, useResizableWidth } from "./Resizer";
import type { IpcError } from "../ipc";

/** 两个 revision 间的「改动文件列表 + 行级 diff」主体(from → to)。
 *  比较页与历史视图的「选 2 提交就地比较」共用,避免重复。
 *  自身是纵向 flex,丢进任意 flex 容器即可铺满。 */
export function ComparePanel({ repo, from, to }: { repo: string; from: string; to: string }) {
  const [file, setFile] = useState<string | null>(null);
  // 换仓库 / 换比较两端时清空选中文件(避免残留到不存在的文件)。
  useEffect(() => { setFile(null); }, [repo, from, to]);

  const filesQ = useCompareFiles(repo, from, to);
  const diffQ = useCompareDiff(repo, from, to, file);
  const files = filesQ.data ?? [];
  const same = !!from && from === to;
  const err = (filesQ.error as IpcError | null)?.message ?? (diffQ.error as IpcError | null)?.message ?? null;

  const col = useResizableWidth("compare.filesW", 288, 200, 640);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {err && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{err}</p>}
      <div className="flex min-h-0 flex-1">
        <div className="flex shrink-0 flex-col overflow-hidden border-r border-line" style={{ width: col.w }}>
          <div className="min-h-0 flex-1 overflow-hidden">
            {same ? (
              <div className="p-3 text-xs text-fg-subtle">两端相同，无差异</div>
            ) : filesQ.isLoading ? (
              <div className="p-3 text-xs text-fg-subtle">加载中…</div>
            ) : files.length === 0 ? (
              <div className="p-3 text-xs text-fg-subtle">无改动</div>
            ) : (
              <CommitFileList files={files} selected={file} onSelect={setFile} />
            )}
          </div>
        </div>
        <Resizer onDown={col.onDown} />
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!file} repo={repo} />
        </main>
      </div>
    </div>
  );
}
