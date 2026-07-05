import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { EditorView, WidgetType, Decoration, lineNumbers, type DecorationSet } from "@codemirror/view";
import { EditorState, StateField, StateEffect, type Text, type Range } from "@codemirror/state";
import { writeResolved, type IpcError } from "../ipc";
import { useConflictSides, useFileText, invalidateWorktree, qk } from "../lib/queries";
import { buildMergeModel, type MergeModel, type MergeRegion, type Spacer } from "../lib/mergeModel";
import { syntaxExtensionForLang } from "../lib/cmSyntax";
import { languageIdForPath } from "../lib/syntax";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { useT } from "../lib/i18n";

const LH = 18; // 行高(px),与下方 theme 里的 line-height 严格一致,占位行高度据此换算

/** 把含冲突标记的工作区文件解析出「初始结果」:保留已自动合并的干净段,冲突段默认取我方。 */
function initialResultFromWorking(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].startsWith("<<<<<<<")) {
      i++;
      const ours: string[] = [];
      while (i < lines.length && !lines[i].startsWith("|||||||") && !lines[i].startsWith("=======")) ours.push(lines[i++]);
      if (i < lines.length && lines[i].startsWith("|||||||")) {
        i++;
        while (i < lines.length && !lines[i].startsWith("=======")) i++;
      }
      if (i < lines.length && lines[i].startsWith("=======")) i++;
      while (i < lines.length && !lines[i].startsWith(">>>>>>>")) i++; // 跳过对方
      if (i < lines.length && lines[i].startsWith(">>>>>>>")) i++;
      out.push(...ours);
    } else {
      out.push(lines[i++]);
    }
  }
  return out.join("\n");
}

// ---- CodeMirror:占位行 / 接受按钮 / 装饰字段 ----

class SpacerWidget extends WidgetType {
  constructor(readonly px: number) {
    super();
  }
  eq(o: SpacerWidget) {
    return o.px === this.px;
  }
  toDOM() {
    const d = document.createElement("div");
    d.className = "cm-merge-spacer";
    d.style.height = `${this.px}px`;
    return d;
  }
  get estimatedHeight() {
    return this.px;
  }
}

/** 结果栏每个变更区域顶部的「用我方 / 用对方」按钮(高度 0,绝对定位悬浮,不影响对齐)。 */
class AcceptWidget extends WidgetType {
  constructor(
    readonly idx: number,
    readonly ours: boolean,
    readonly theirs: boolean,
    readonly conflict: boolean,
    readonly onPick: (idx: number, side: "o" | "t") => void,
    readonly oursLabel: string,
    readonly theirsLabel: string,
  ) {
    super();
  }
  eq(o: AcceptWidget) {
    return o.idx === this.idx && o.ours === this.ours && o.theirs === this.theirs && o.conflict === this.conflict && o.oursLabel === this.oursLabel && o.theirsLabel === this.theirsLabel;
  }
  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-merge-actions";
    const bar = document.createElement("div");
    bar.className = "bar";
    const mk = (label: string, side: "o" | "t", cls: string) => {
      const b = document.createElement("button");
      b.textContent = label;
      b.className = `act ${cls}`;
      b.onmousedown = (e) => e.preventDefault();
      b.onclick = (e) => {
        e.preventDefault();
        this.onPick(this.idx, side);
      };
      return b;
    };
    if (this.ours) bar.appendChild(mk(this.oursLabel, "o", "ours"));
    if (this.theirs) bar.appendChild(mk(this.theirsLabel, "t", "theirs"));
    wrap.appendChild(bar);
    return wrap;
  }
  get estimatedHeight() {
    return 0;
  }
  ignoreEvent() {
    return true;
  }
}

const setDecos = StateEffect.define<DecorationSet>();
const decoField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(v, tr) {
    v = v.map(tr.changes);
    for (const e of tr.effects) if (e.is(setDecos)) v = e.value;
    return v;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const baseTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "12px", backgroundColor: "var(--color-canvas)", color: "var(--color-fg)" },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace", lineHeight: `${LH}px`, overflow: "auto" },
  ".cm-content": { lineHeight: `${LH}px` },
  ".cm-line": { padding: "0 8px" },
  ".cm-gutters": { backgroundColor: "var(--color-canvas)", color: "var(--color-fg-subtle)", border: "none" },
  ".cm-gutter": { minWidth: "0" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 6px 0 8px" },
  "&.cm-focused": { outline: "none" },
  ".cm-hl-ours": { backgroundColor: "color-mix(in srgb, var(--color-success) 16%, transparent)" },
  ".cm-hl-theirs": { backgroundColor: "color-mix(in srgb, var(--color-accent) 16%, transparent)" },
  ".cm-hl-conflict": { backgroundColor: "color-mix(in srgb, var(--color-warning) 18%, transparent)" },
  ".cm-merge-spacer": { backgroundColor: "color-mix(in srgb, var(--color-fg-subtle) 8%, transparent)", borderBottom: "1px dashed var(--color-line)" },
  ".cm-merge-actions": { position: "relative", height: "0" },
  ".cm-merge-actions .bar": { position: "absolute", right: "8px", top: "-1px", display: "flex", gap: "4px", zIndex: "5" },
  ".cm-merge-actions .act": {
    cursor: "pointer",
    borderRadius: "4px",
    border: "1px solid var(--color-line-strong)",
    padding: "0 6px",
    fontSize: "10px",
    lineHeight: "16px",
    background: "var(--color-elevated)",
    color: "var(--color-fg-muted)",
  },
  ".cm-merge-actions .act:hover": { background: "var(--color-overlay)", color: "var(--color-fg)" },
  ".cm-merge-actions .act.ours:hover": { borderColor: "var(--color-success)", color: "var(--color-success)" },
  ".cm-merge-actions .act.theirs:hover": { borderColor: "var(--color-accent)", color: "var(--color-accent)" },
});

function spacerRange(doc: Text, sp: Spacer): Range<Decoration> {
  const w = new SpacerWidget(sp.count * LH);
  if (sp.afterLine < 0) return Decoration.widget({ widget: w, block: true, side: -1 }).range(0);
  const ln = Math.min(sp.afterLine, doc.lines - 1);
  return Decoration.widget({ widget: w, block: true, side: 1 }).range(doc.line(ln + 1).to);
}

function lineHi(doc: Text, from: number, to: number, cls: string): Range<Decoration>[] {
  const out: Range<Decoration>[] = [];
  const deco = Decoration.line({ class: cls });
  for (let l = from; l < to && l < doc.lines; l++) out.push(deco.range(doc.line(l + 1).from));
  return out;
}

/** 三栏合并冲突编辑器(对标 JetBrains):左=我方(只读)/ 中=可编辑结果 / 右=对方(只读)。 */
export function ConflictEditor({ repo, file }: { repo: string; file: string }) {
  const t = useT();
  const qc = useQueryClient();
  const toast = useToast();
  const sidesQ = useConflictSides(repo, file);
  const workingQ = useFileText(repo, file, true);

  const oursStr = sidesQ.data?.ours ?? "";
  const theirsStr = sidesQ.data?.theirs ?? "";
  const baseStr = sidesQ.data?.base ?? null;
  const initialResult = useMemo(
    () => (workingQ.data != null ? initialResultFromWorking(workingQ.data) : ""),
    [workingQ.data],
  );
  const ready = sidesQ.data != null && workingQ.data != null;

  const oursRef = useRef<HTMLDivElement>(null);
  const resultRef = useRef<HTMLDivElement>(null);
  const theirsRef = useRef<HTMLDivElement>(null);
  const resultViewRef = useRef<EditorView | null>(null);
  const modelRef = useRef<MergeModel | null>(null);

  const [remaining, setRemaining] = useState(0);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!ready || !oursRef.current || !resultRef.current || !theirsRef.current) return;

    const oursLines = oursStr.split("\n");
    const theirsLines = theirsStr.split("\n");
    const syntaxExtensions = syntaxExtensionForLang(languageIdForPath(file));

    const readOnly = [EditorState.readOnly.of(true), EditorView.editable.of(false)];

    const handlePick = (idx: number, side: "o" | "t") => {
      const model = modelRef.current;
      const view = resultViewRef.current;
      if (!model || !view) return;
      const r = model.regions[idx];
      if (!r) return;
      const src = side === "o" ? oursLines : theirsLines;
      const [sf, st] = side === "o" ? [r.oFrom, r.oTo] : [r.tFrom, r.tTo];
      const seg = src.slice(sf, st);
      const doc = view.state.doc;
      const lc = doc.lines;
      const from = r.rFrom < lc ? doc.line(r.rFrom + 1).from : doc.length;
      const to = r.rTo < lc ? doc.line(r.rTo + 1).from : doc.length;
      let insert = seg.join("\n");
      if (r.rTo < lc) insert += "\n"; // 区域含末行换行,补回以保持行结构
      view.dispatch({ changes: { from, to, insert } });
    };

    const buildSide = (doc: Text, spacers: Spacer[], regions: MergeRegion[], which: "o" | "t") => {
      const ranges: Range<Decoration>[] = spacers.map((sp) => spacerRange(doc, sp));
      for (const r of regions) {
        if (which === "o" && r.oursChanged) ranges.push(...lineHi(doc, r.oFrom, r.oTo, "cm-hl-ours"));
        if (which === "t" && r.theirsChanged) ranges.push(...lineHi(doc, r.tFrom, r.tTo, "cm-hl-theirs"));
      }
      return Decoration.set(ranges, true);
    };

    const buildResult = (doc: Text, model: MergeModel) => {
      const ranges: Range<Decoration>[] = model.spacersResult.map((sp) => spacerRange(doc, sp));
      model.regions.forEach((r, idx) => {
        const cls = r.conflict ? "cm-hl-conflict" : r.oursChanged ? "cm-hl-ours" : "cm-hl-theirs";
        ranges.push(...lineHi(doc, r.rFrom, r.rTo, cls));
        const pos = r.rFrom < doc.lines ? doc.line(r.rFrom + 1).from : doc.length;
        // 真冲突两侧按钮都给;单边变更只给那一边。
        ranges.push(
          Decoration.widget({
            widget: new AcceptWidget(idx, r.oursChanged || r.conflict, r.theirsChanged || r.conflict, r.conflict, handlePick, t("conflictEd.useOurs"), t("conflictEd.useTheirs")),
            block: true,
            side: -1,
          }).range(pos),
        );
      });
      return Decoration.set(ranges, true);
    };

    // 占位:在三个视图创建后再赋真身(rv 的 updateListener 会回调它)。
    let recompute = () => {};

    const ov = new EditorView({
      doc: oursStr,
      parent: oursRef.current,
      extensions: [lineNumbers(), baseTheme, ...syntaxExtensions, decoField, ...readOnly],
    });
    const tv = new EditorView({
      doc: theirsStr,
      parent: theirsRef.current,
      extensions: [lineNumbers(), baseTheme, ...syntaxExtensions, decoField, ...readOnly],
    });
    const rv = new EditorView({
      doc: initialResult,
      parent: resultRef.current,
      extensions: [
        lineNumbers(),
        baseTheme,
        ...syntaxExtensions,
        decoField,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) recompute();
        }),
      ],
    });
    resultViewRef.current = rv;

    recompute = () => {
      const model = buildMergeModel(oursStr, rv.state.doc.toString(), theirsStr, baseStr);
      modelRef.current = model;
      ov.dispatch({ effects: setDecos.of(buildSide(ov.state.doc, model.spacersOurs, model.regions, "o")) });
      tv.dispatch({ effects: setDecos.of(buildSide(tv.state.doc, model.spacersTheirs, model.regions, "t")) });
      rv.dispatch({ effects: setDecos.of(buildResult(rv.state.doc, model)) });
      setRemaining(model.regions.filter((r) => r.conflict).length);
    };

    recompute();

    // 三栏同步滚动(任一栏滚动,其余跟随)。
    const views = [ov, rv, tv];
    let lock = false;
    const onScroll = (src: EditorView) => () => {
      if (lock) return;
      lock = true;
      const { scrollTop, scrollLeft } = src.scrollDOM;
      for (const v of views)
        if (v !== src) {
          v.scrollDOM.scrollTop = scrollTop;
          v.scrollDOM.scrollLeft = scrollLeft;
        }
      lock = false;
    };
    const handlers = views.map((v) => {
      const fn = onScroll(v);
      v.scrollDOM.addEventListener("scroll", fn);
      return [v, fn] as const;
    });

    return () => {
      handlers.forEach(([v, fn]) => v.scrollDOM.removeEventListener("scroll", fn));
      ov.destroy();
      tv.destroy();
      rv.destroy();
      resultViewRef.current = null;
      modelRef.current = null;
    };
  }, [ready, repo, file, oursStr, theirsStr, baseStr, initialResult]);

  if (sidesQ.isLoading || workingQ.isLoading) return <Center>{t("conflictEd.loading")}</Center>;
  if (sidesQ.error || workingQ.error) return <Center>{t("conflictEd.readFailed")}</Center>;

  const replaceAll = (text: string) => {
    const v = resultViewRef.current;
    if (!v) return;
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
  };

  async function apply() {
    const v = resultViewRef.current;
    if (!v) return;
    setBusy(true);
    try {
      await writeResolved(repo, file, v.state.doc.toString());
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: t("conflictEd.resolved") });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs">
        <span className={remaining > 0 ? "text-warning" : "text-fg-muted"}>
          {remaining > 0 ? t("conflictEd.nConflicts", { n: remaining }) : t("conflictEd.noConflict")}
        </span>
        <span className="text-fg-subtle">{t("conflictEd.hint")}</span>
        <div className="ml-auto flex items-center gap-1.5">
          <Button variant="ghost" size="sm" onClick={() => replaceAll(oursStr)}>
            {t("conflictEd.allOurs")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => replaceAll(theirsStr)}>
            {t("conflictEd.allTheirs")}
          </Button>
          <Button
            variant="commit"
            size="sm"
            onClick={apply}
            disabled={busy}
            title={t("conflictEd.applyTitle")}
          >
            {t("conflictEd.apply")}
          </Button>
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <Pane label={t("conflictEd.paneOurs")} tint="text-success" forwardRef={oursRef} />
        <div className="w-px shrink-0 bg-line" />
        <Pane label={t("conflictEd.paneResult")} tint="text-fg" forwardRef={resultRef} />
        <div className="w-px shrink-0 bg-line" />
        <Pane label={t("conflictEd.paneTheirs")} tint="text-accent" forwardRef={theirsRef} />
      </div>
    </div>
  );
}

function Pane({ label, tint, forwardRef }: { label: string; tint: string; forwardRef: React.RefObject<HTMLDivElement | null> }) {
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className={`shrink-0 border-b border-line bg-elevated px-2 py-1 text-[11px] font-medium ${tint}`}>{label}</div>
      <div ref={forwardRef} className="min-h-0 flex-1 overflow-hidden" />
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">{children}</div>;
}
