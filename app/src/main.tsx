import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./index.css";
import App from "./App";
import { ToastProvider } from "./components/Toast";
import { applyTheme, getStoredTheme, applyGlassMode } from "./lib/theme";

// 渲染前先定主题,避免首屏从浅色「闪」到暗色
applyTheme(getStoredTheme());
// 渲染前定玻璃档位,写到 <html data-glass>
applyGlassMode();

function GlassFilter() {
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden>
      <filter id="lgWarp" x="-20%" y="-20%" width="140%" height="140%" colorInterpolationFilters="sRGB">
        <feTurbulence type="fractalNoise" baseFrequency="0.008 0.012" numOctaves={2} seed={7} result="noise" />
        <feGaussianBlur in="noise" stdDeviation={1.4} result="snoise" />
        <feDisplacementMap in="SourceGraphic" in2="snoise" scale={30} xChannelSelector="R" yChannelSelector="G" />
      </filter>
    </svg>
  );
}

// git 读多为本地、变化由 watcher/失效驱动:关掉窗口聚焦自动重取,
// 失败不重试(git 错误重试无意义),数据短时间内视为新鲜。
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 5_000, refetchOnWindowFocus: false, retry: false },
  },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <>
          <GlassFilter />
          <App />
        </>
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
