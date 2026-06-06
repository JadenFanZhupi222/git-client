import { useFileText } from "../lib/queries";

/** 冲突文件只读视图:高亮 <<<<<<< / ======= / >>>>>>> 标记,
 *  我方区(绿)/ 对方区(蓝)/ base 区(灰)分色,便于人工核对。 */
export function ConflictView({ repo, file }: { repo: string; file: string }) {
  const q = useFileText(repo, file, true);

  if (q.isLoading) return <Center>加载中…</Center>;
  if (q.error || q.data == null) return <Center>无法读取文件</Center>;

  const lines = q.data.split("\n");
  let region: "none" | "ours" | "base" | "theirs" = "none";

  return (
    <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
      {lines.map((line, i) => {
        let cls = "text-fg";
        if (line.startsWith("<<<<<<<")) {
          region = "ours";
          cls = "bg-danger/15 font-semibold text-danger";
        } else if (line.startsWith("|||||||")) {
          region = "base";
          cls = "bg-overlay font-semibold text-fg-muted";
        } else if (line.startsWith("=======") && region !== "none") {
          region = "theirs";
          cls = "bg-overlay font-semibold text-fg-muted";
        } else if (line.startsWith(">>>>>>>")) {
          cls = "bg-danger/15 font-semibold text-danger";
          region = "none";
        } else if (region === "ours") {
          cls = "bg-success/10 text-fg";
        } else if (region === "theirs") {
          cls = "bg-accent/10 text-fg";
        } else if (region === "base") {
          cls = "bg-overlay/50 text-fg-muted";
        }
        return (
          <div key={i} className={`whitespace-pre px-3 ${cls}`}>
            {line || " "}
          </div>
        );
      })}
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
