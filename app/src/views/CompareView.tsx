import { useEffect, useState } from "react";
import { useRefs, useCurrentBranch, useCompareFiles } from "../lib/queries";
import { ComparePanel } from "../components/ComparePanel";
import { BranchIcon } from "../components/icons";
import { useT } from "../lib/i18n";

const PICK = "field rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg";

/** revision 选择器。必须定义在模块顶层 —— 若嵌套在 CompareView 内,每次渲染都是
 *  新组件类型,React 会卸载重挂里面的原生 <select>,WebKit 下会丢掉首次 change
 *  事件(表现为"要选两遍才生效")。 */
function RevSelect({ value, onChange, label, names }: {
  value: string; onChange: (v: string) => void; label: string; names: string[];
}) {
  const t = useT();
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={PICK} title={label} aria-label={label}>
      {/* value 为空时显示占位项,避免空状态被首个分支「冒充」(选中显示项不触发 change)。 */}
      {!value && <option value="" disabled>{t("compare.pickBranch")}</option>}
      {/* 当前值若不在候选里(如填了 SHA),仍展示出来 */}
      {value && !names.includes(value) && <option value={value}>{value}</option>}
      {names.map((n) => <option key={n} value={n}>{n}</option>)}
    </select>
  );
}

/** 比较两个 revision(分支/标签/提交)的改动:from → to。
 *  顶部两个分支选择器,左列改动文件,右侧行级 diff。 */
export function CompareView({ repo }: { repo: string }) {
  const t = useT();
  const refsQ = useRefs(repo, !!repo);
  const curQ = useCurrentBranch(repo);
  // 选择器候选:本地分支 + 远程跟踪分支 + 标签(去掉 HEAD 这种符号引用)。
  const refNames = (refsQ.data ?? [])
    .filter((r) => r.kind === "local" || r.kind === "remote" || r.kind === "tag")
    .map((r) => r.name);

  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");

  // 两端默认 = 当前分支;切仓库 / 切分支时复位。
  // ⚠️ 必须是单个 effect:之前拆成「默认」+「切仓库清空」两个,挂载时清空 effect
  // 后执行,把默认值抹成空 → 下拉框显示分支但 state 为空 → enabled=false 不发起比较
  //(表现为"首次进入空、要把两个分支都重选一遍")。合并后顺序无歧义。
  useEffect(() => {
    const cur = curQ.data ?? "";
    setFrom(cur);
    setTo(cur);
  }, [repo, curQ.data]);

  // 计数仅用于工具栏文案;与 ComparePanel 同 key,React Query 复用缓存不重复请求。
  const files = useCompareFiles(repo, from, to).data ?? [];
  const same = !!from && from === to;
  // 编辑性统计头用的总增删
  const totalAdd = files.reduce((s, f) => s + f.additions, 0);
  const totalDel = files.reduce((s, f) => s + f.deletions, 0);

  return (
    <div className="flex h-full flex-col">
      {/* 选择器工具栏:ref 药丸 从 → 到 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2 text-xs text-fg-muted">
        <BranchIcon width={13} height={13} />
        <RevSelect value={from} onChange={setFrom} label="基准(from)" names={refNames} />
        <span className="text-fg-subtle">→</span>
        <RevSelect value={to} onChange={setTo} label="对比(to)" names={refNames} />
      </div>

      {/* 编辑性统计头:大号衬线 +adds / −dels + 一句概述 */}
      {!same && files.length > 0 && (
        <div className="shrink-0 border-b border-line px-4 py-3">
          <div className="serif flex items-baseline gap-3.5 leading-none">
            <span className="text-[34px] text-success">+{totalAdd}</span>
            <span className="text-[34px] text-danger">−{totalDel}</span>
          </div>
          <p className="mt-2 text-xs text-fg-muted">
            <span className="font-mono text-fg">{to}</span> {t("compare.vs")} <span className="font-mono text-fg">{from}</span>
            {" · "}
            {files.length} {t("compare.filesChanged")}
          </p>
        </div>
      )}
      {same && (
        <div className="shrink-0 border-b border-line px-4 py-2.5 text-[11px] text-fg-subtle">
          {t("compare.same")}
        </div>
      )}

      <ComparePanel repo={repo} from={from} to={to} />
    </div>
  );
}
