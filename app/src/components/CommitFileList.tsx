import { type FileChangeDto } from "../ipc";

const COLOR: Record<string, string> = {
  added: "text-[#3fb950]", modified: "text-[#58a6ff]", deleted: "text-[#f85149]", renamed: "text-[#d29922]",
};
const LETTER: Record<string, string> = { added: "A", modified: "M", deleted: "D", renamed: "R" };

export function CommitFileList({
  files, selected, onSelect,
}: { files: FileChangeDto[]; selected: string | null; onSelect: (path: string) => void }) {
  if (files.length === 0) return <div className="p-3 text-xs text-[#6e7681]">无改动文件</div>;
  return (
    <div className="overflow-y-auto">
      {files.map((f) => (
        <div key={f.path} onClick={() => onSelect(f.path)}
          className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer font-mono text-sm ${selected === f.path ? "bg-[#161b22]" : "hover:bg-[#161b22]"}`}>
          <span className={`w-4 ${COLOR[f.status] ?? "text-[#8b949e]"}`}>{LETTER[f.status] ?? "?"}</span>
          <span className="truncate text-[#c9d1d9]">{f.path}</span>
        </div>
      ))}
    </div>
  );
}
