import { useEffect, useState } from "react";

/** 可拖拽列宽:持久化到 localStorage,夹在 [min,max] 之间。
 *  返回当前宽度 w(px)和拖拽起手 onDown(绑到分隔条 onMouseDown)。 */
export function useResizableWidth(key: string, initial: number, min: number, max: number) {
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const [w, setW] = useState(() => {
    const saved = localStorage.getItem(key);
    const n = saved ? parseInt(saved, 10) : NaN;
    return Number.isFinite(n) ? clamp(n) : initial;
  });
  useEffect(() => { localStorage.setItem(key, String(w)); }, [key, w]);

  const onDown = (e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = w;
    const move = (ev: MouseEvent) => setW(clamp(startW + ev.clientX - startX));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  return { w, onDown };
}

/** 列间分隔条:视觉 1px 线 + 更宽的透明命中区(悬停高亮)。 */
export function Resizer({ onDown }: { onDown: (e: React.MouseEvent) => void }) {
  return (
    <div onMouseDown={onDown} className="group relative w-px shrink-0 cursor-col-resize bg-line">
      <div className="absolute inset-y-0 -left-1 -right-1 z-10 transition-colors group-hover:bg-accent/40" />
    </div>
  );
}
