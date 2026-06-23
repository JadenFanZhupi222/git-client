import { useEffect, useState, type ReactNode } from "react";
import { useCompareFiles, useCompareDiff } from "../lib/queries";
import { CommitFileList } from "./CommitFileList";
import { DiffView } from "./DiffView";
import { FloatBar, FLOAT_BAR_INSET } from "./ui/FloatBar";
import { Resizer, useResizableWidth } from "./Resizer";
import type { IpcError } from "../ipc";
import { useT } from "../lib/i18n";

/** 两个 revision 间的「改动文件列表 + 行级 diff」主体(from → to)。
 *  比较页与历史视图的「选 2 提交就地比较」共用,避免重复。
 *  自身是纵向 flex,丢进任意 flex 容器即可铺满。
 *
 *  可选 `toolbar` / `statHead`:比较页传入,把浮动玻璃工具栏(ref 药丸 + 交换)与编辑性统计头
 *  收进左列(对齐 Strata 原型);历史内联比较两者都不传,沿用朴素布局。 */
export function ComparePanel({
  repo, from, to, toolbar, statHead, listWidthKey = "compare.filesW",
}: {
  repo: string; from: string; to: string;
  toolbar?: ReactNode; statHead?: ReactNode; listWidthKey?: string;
}) {
  const t = useT();
  const [file, setFile] = useState<string | null>(null);
  // 换仓库 / 换比较两端时清空选中文件(避免残留到不存在的文件)。
  useEffect(() => { setFile(null); }, [repo, from, to]);

  const filesQ = useCompareFiles(repo, from, to);
  const diffQ = useCompareDiff(repo, from, to, file);
  const files = filesQ.data ?? [];
  const same = !!from && from === to;
  const err = (filesQ.error as IpcError | null)?.message ?? (diffQ.error as IpcError | null)?.message ?? null;

  const col = useResizableWidth(listWidthKey, toolbar ? 340 : 288, 200, 640);

  const list = same ? (
    <div className="p-3 text-xs text-fg-subtle">{t("comparePanel.same")}</div>
  ) : filesQ.isLoading ? (
    <div className="p-3 text-xs text-fg-subtle">{t("common.loading")}</div>
  ) : files.length === 0 ? (
    <div className="p-3 text-xs text-fg-subtle">{t("comparePanel.noChanges")}</div>
  ) : (
    <CommitFileList files={files} selected={file} onSelect={setFile} />
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {err && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{err}</p>}
      <div className="flex min-h-0 flex-1">
        <div className="relative flex shrink-0 flex-col overflow-hidden border-r border-line" style={{ width: col.w }}>
          {toolbar ? (
            <>
              {/* 浮动玻璃工具栏(ref 药丸 + 交换);文件列表从其下穿过显折射。 */}
              <FloatBar>{toolbar}</FloatBar>
              <div className="flex min-h-0 flex-1 flex-col" style={{ paddingTop: FLOAT_BAR_INSET }}>
                {statHead}
                <div className="min-h-0 flex-1 overflow-hidden">{list}</div>
              </div>
            </>
          ) : (
            <div className="min-h-0 flex-1 overflow-hidden">{list}</div>
          )}
        </div>
        <Resizer onDown={col.onDown} />
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!file} repo={repo} />
        </main>
      </div>
    </div>
  );
}
