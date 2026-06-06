import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AlertIcon, CheckIcon, CloseIcon } from "./icons";

/**
 * 可复用 toast 通知系统。
 *
 * 用法:在根部包一层 <ToastProvider>,任意组件里 `const toast = useToast()`,
 * 调 `toast({ kind, title, detail? })` 即可。成功/信息 4s 自动消失,错误保留到手动关。
 * 视觉走设计 token:图标 + 标题 + mono 细节、圆角描边、柔和阴影、右下滑入/滑出。
 */

export type ToastKind = "success" | "error" | "info";

export interface ToastInput {
  kind: ToastKind;
  title: string;
  detail?: string;
  /** 自动消失毫秒;0 = 不自动消失。默认:error=0(常驻),其余=4000。 */
  duration?: number;
}

interface ToastItem extends ToastInput {
  id: number;
  leaving?: boolean;
}

const ToastCtx = createContext<(t: ToastInput) => void>(() => {});

/** 在任意组件里取得 push 函数:`const toast = useToast(); toast({...})`。 */
export function useToast() {
  return useContext(ToastCtx);
}

const KIND_STYLE: Record<ToastKind, { cls: string; icon: ReactNode }> = {
  success: { cls: "text-success", icon: <CheckIcon width={15} height={15} /> },
  error: { cls: "text-danger", icon: <AlertIcon width={15} height={15} /> },
  info: { cls: "text-accent", icon: <AlertIcon width={15} height={15} /> },
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const idRef = useRef(0);

  const remove = useCallback((id: number) => {
    setItems((xs) => xs.filter((x) => x.id !== id));
  }, []);

  // 先播放滑出动画,再从列表移除。
  const dismiss = useCallback(
    (id: number) => {
      setItems((xs) => xs.map((x) => (x.id === id ? { ...x, leaving: true } : x)));
      setTimeout(() => remove(id), 180);
    },
    [remove],
  );

  const push = useCallback(
    (t: ToastInput) => {
      idRef.current += 1;
      const id = idRef.current;
      setItems((xs) => [...xs, { ...t, id }]);
      const duration = t.duration ?? (t.kind === "error" ? 0 : 4000);
      if (duration > 0) setTimeout(() => dismiss(id), duration);
    },
    [dismiss],
  );

  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="pointer-events-none fixed bottom-3 right-3 z-[60] flex flex-col gap-2">
        {items.map((t) => (
          <ToastCard key={t.id} item={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

function ToastCard({ item, onDismiss }: { item: ToastItem; onDismiss: () => void }) {
  const style = KIND_STYLE[item.kind];
  return (
    <div
      className={`pointer-events-auto flex w-80 items-start gap-2.5 rounded-lg border border-line-strong bg-elevated p-3 shadow-lg ${
        item.leaving ? "toast-out" : "toast-in"
      }`}
    >
      <span className={`mt-0.5 shrink-0 ${style.cls}`}>{style.icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-fg">{item.title}</div>
        {item.detail && (
          <div className="mt-0.5 break-words whitespace-pre-line font-mono text-[11px] leading-snug text-fg-muted">
            {item.detail}
          </div>
        )}
      </div>
      <button
        onClick={onDismiss}
        title="关闭"
        className="-mr-0.5 -mt-0.5 shrink-0 rounded p-1 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg"
      >
        <CloseIcon width={12} height={12} />
      </button>
    </div>
  );
}
