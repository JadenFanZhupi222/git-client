import { useEffect, useState } from "react";
import { useRefs, useCurrentBranch, useCompareFiles } from "../lib/queries";
import { ComparePanel } from "../components/ComparePanel";
import { useT } from "../lib/i18n";

/** revision 选择器,样式为 ref 药丸(从/到 眉签 + mono 值 + chevron),但内核仍是原生 <select>。
 *  必须定义在模块顶层 —— 若嵌套在 CompareView 内,每次渲染都是新组件类型,React 会卸载重挂
 *  里面的原生 <select>,WebKit 下会丢掉首次 change 事件(表现为"要选两遍才生效")。 */
function RevSelect({ value, onChange, kind, names }: {
  value: string; onChange: (v: string) => void; kind: string; names: string[];
}) {
  const t = useT();
  return (
    <div className="flex shrink-0 items-center gap-1.5 rounded-lg border border-line bg-canvas/50 px-2.5 py-1.5">
      <span className="text-[9px] font-semibold uppercase tracking-[0.05em] text-fg-subtle">{kind}</span>
      <div className="relative flex items-center">
        <select
          value={value}
          onChange={(e) => onChange(e.target.value)}
          title={kind}
          aria-label={kind}
          className="max-w-[120px] cursor-pointer appearance-none truncate bg-transparent pr-4 font-mono text-[12.5px] font-medium text-fg focus:outline-none"
        >
          {/* value 为空时显示占位项,避免空状态被首个分支「冒充」(选中显示项不触发 change)。 */}
          {!value && <option value="" disabled>{t("compare.pickBranch")}</option>}
          {/* 当前值若不在候选里(如填了 SHA),仍展示出来 */}
          {value && !names.includes(value) && <option value={value}>{value}</option>}
          {names.map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
        <svg width={10} height={10} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round"
          className="pointer-events-none absolute right-0 text-fg-subtle">
          <path d="M4 6l4 4 4-4" />
        </svg>
      </div>
    </div>
  );
}

/** 比较两个 revision(分支/标签/提交)的改动:from → to。
 *  左列浮动玻璃工具栏(ref 药丸 + 交换)+ 编辑性统计头 + 改动文件,右侧行级 diff。 */
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

  // 计数仅用于统计头;与 ComparePanel 同 key,React Query 复用缓存不重复请求。
  const files = useCompareFiles(repo, from, to).data ?? [];
  const same = !!from && from === to;
  const totalAdd = files.reduce((s, f) => s + f.additions, 0);
  const totalDel = files.reduce((s, f) => s + f.deletions, 0);

  const toolbar = (
    <>
      <RevSelect value={from} onChange={setFrom} kind={t("compare.from")} names={refNames} />
      <svg width={14} height={14} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" className="shrink-0 text-fg-subtle">
        <path d="M3 8h10M9 4l4 4-4 4" />
      </svg>
      <RevSelect value={to} onChange={setTo} kind={t("compare.to")} names={refNames} />
      <button
        onClick={() => { setFrom(to); setTo(from); }}
        title={t("compare.swap")}
        aria-label={t("compare.swap")}
        className="ml-auto grid h-7 w-7 shrink-0 place-items-center rounded-md border border-line text-fg-muted transition-colors hover:text-fg"
      >
        <svg width={13} height={13} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
          <path d="M4 5h8l-2-2M12 11H4l2 2" />
        </svg>
      </button>
    </>
  );

  // 编辑性统计头:大号衬线 +adds / −dels + 一句概述。移入左列滚动体顶部(对齐原型)。
  const statHead = same ? (
    <div className="shrink-0 border-b border-line px-4 py-2.5 text-[11px] text-fg-subtle">{t("compare.same")}</div>
  ) : files.length > 0 ? (
    <div className="shrink-0 border-b border-line px-4 pb-3.5 pt-2">
      <div className="serif flex items-baseline gap-3 leading-none">
        <span className="text-[30px] text-success">+{totalAdd}</span>
        <span className="text-[30px] text-danger">−{totalDel}</span>
      </div>
      <p className="mt-2 text-[12.5px] text-fg-muted">
        <span className="font-mono text-fg">{to}</span> {t("compare.vs")} <span className="font-mono text-fg">{from}</span>
        {" · "}
        {files.length} {t("compare.filesChanged")}
      </p>
    </div>
  ) : null;

  return (
    <div className="flex h-full flex-col">
      <ComparePanel repo={repo} from={from} to={to} toolbar={toolbar} statHead={statHead} />
    </div>
  );
}
