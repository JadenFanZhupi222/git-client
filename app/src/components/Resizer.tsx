import { useEffect, useState } from "react";

/** 可拖拽列宽:持久化到 localStorage,夹在 [min,max] 之间。
 *  返回当前宽度 w(px)和拖拽起手 onDown(绑到分隔条 onMouseDown)。 */
export function useResizableWidth(key: string, initial: number, min: number, max: number) {
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const [w, setW] = useState(() => {
    const saved = localStorage.getItem(key);
    const n = saved ? parseInt(saved, 10) : NaN;
    return Number.isFinite(n) ? clamp(n) : clamp(initial);
  });
  useEffect(() => { setW((current) => Math.min(max, Math.max(min, current))); }, [min, max]);
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

  const reset = () => setW(clamp(initial));
  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 48 : 16;
    if (e.key === "Home") {
      e.preventDefault();
      reset();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      setW((current) => clamp(current - step));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      setW((current) => clamp(current + step));
    }
  };

  return { w, min, max, onDown, onKeyDown, reset };
}

/** 列间分隔条:视觉 1px 线 + 更宽的透明命中区(悬停高亮)。 */
export function Resizer({
  value,
  min,
  max,
  label,
  onDown,
  onKeyDown,
  onReset,
}: {
  value: number;
  min: number;
  max: number;
  label: string;
  onDown: (e: React.MouseEvent) => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
  onReset: () => void;
}) {
  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      onMouseDown={onDown}
      onKeyDown={onKeyDown}
      onDoubleClick={onReset}
      className="group relative w-px shrink-0 cursor-col-resize bg-line focus-visible:bg-accent focus-visible:outline-offset-[-2px]"
    >
      <div className="absolute inset-y-0 -left-1 -right-1 z-10 transition-colors group-hover:bg-accent/40" />
    </div>
  );
}
