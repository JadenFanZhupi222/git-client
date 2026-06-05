import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { TabBar, type Tab } from "./components/TabBar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("changes");

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir === "string") setRepo(dir);
  }

  return (
    <main className="min-h-screen bg-[#0d1117] text-[#e6edf3] font-sans">
      <header className="flex items-center gap-3 px-4 py-3 border-b border-[#21262d]">
        <h1 className="text-lg font-bold">Git 客户端</h1>
        <button className="text-sm px-2 py-1 rounded bg-[#21262d] text-[#c9d1d9]" onClick={pickRepo}>选择仓库</button>
        {repo && <span className="text-xs text-[#8b949e] font-mono truncate">{repo}</span>}
      </header>
      {repo && <TabBar active={tab} onChange={setTab} />}
      {repo
        ? (tab === "changes" ? <ChangesView repo={repo} /> : <HistoryView repo={repo} />)
        : <p className="p-6 text-[#8b949e]">选择一个本地 git 仓库开始。</p>}
    </main>
  );
}
