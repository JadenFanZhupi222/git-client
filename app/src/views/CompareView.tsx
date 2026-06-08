import { useEffect, useState } from "react";
import { useRefs, useCurrentBranch, useCompareFiles } from "../lib/queries";
import { ComparePanel } from "../components/ComparePanel";
import { BranchIcon } from "../components/icons";

const PICK = "rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg focus:border-accent focus:outline-none";

/** revision 选择器。必须定义在模块顶层 —— 若嵌套在 CompareView 内,每次渲染都是
 *  新组件类型,React 会卸载重挂里面的原生 <select>,WebKit 下会丢掉首次 change
 *  事件(表现为"要选两遍才生效")。 */
function RevSelect({ value, onChange, label, names }: {
  value: string; onChange: (v: string) => void; label: string; names: string[];
}) {
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={PICK} title={label} aria-label={label}>
      {/* 当前值若不在候选里(如填了 SHA),仍展示出来 */}
      {value && !names.includes(value) && <option value={value}>{value}</option>}
      {names.map((n) => <option key={n} value={n}>{n}</option>)}
    </select>
  );
}

/** 比较两个 revision(分支/标签/提交)的改动:from → to。
 *  顶部两个分支选择器,左列改动文件,右侧行级 diff。 */
export function CompareView({ repo }: { repo: string }) {
  const refsQ = useRefs(repo, !!repo);
  const curQ = useCurrentBranch(repo);
  // 选择器候选:本地分支 + 远程跟踪分支 + 标签(去掉 HEAD 这种符号引用)。
  const refNames = (refsQ.data ?? [])
    .filter((r) => r.kind === "local" || r.kind === "remote" || r.kind === "tag")
    .map((r) => r.name);

  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");

  // 默认两端都设为当前分支(用户改 from 即可看与某分支的差异)
  useEffect(() => {
    const cur = curQ.data ?? "";
    if (cur) {
      setFrom((p) => p || cur);
      setTo((p) => p || cur);
    }
  }, [curQ.data]);
  // 切仓库重置
  useEffect(() => { setFrom(""); setTo(""); }, [repo]);

  // 计数仅用于工具栏文案;与 ComparePanel 同 key,React Query 复用缓存不重复请求。
  const files = useCompareFiles(repo, from, to).data ?? [];
  const same = !!from && from === to;

  return (
    <div className="flex h-full flex-col">
      {/* 选择器工具栏 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2 text-xs text-fg-muted">
        <BranchIcon width={13} height={13} />
        <RevSelect value={from} onChange={setFrom} label="基准(from)" names={refNames} />
        <span className="text-fg-subtle">→</span>
        <RevSelect value={to} onChange={setTo} label="对比(to)" names={refNames} />
        <span className="ml-2 text-[11px] text-fg-subtle">
          {same ? "选择两个不同的分支查看差异" : `${files.length} 个文件改动`}
        </span>
      </div>

      <ComparePanel repo={repo} from={from} to={to} />
    </div>
  );
}
