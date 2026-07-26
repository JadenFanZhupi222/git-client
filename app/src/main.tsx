import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
// 自托管 Geist 可变字体(离线打包,不依赖网络)。拉丁字母/数字走 Geist,
// 中文无 Geist 字形 → 逐字回退到字体栈里的 YaHei/system(见 index.css 的 --font-*)。
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
// Instrument Serif:仅「编辑性大字时刻」用(启动屏巨字、提交标题、视图标题、blame 文件名)。
// 只 400 + italic,自托管离线可用。中文衬线由字体栈回退到 Noto Serif SC(index.html 的 <link>)。
import "@fontsource/instrument-serif/400.css";
import "@fontsource/instrument-serif/400-italic.css";
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

async function bootstrap() {
  // The bridge is excluded from normal production builds; only the dedicated
  // `vite --mode e2e` bundle imports and initializes it.
  if (import.meta.env.MODE === "e2e") {
    await import("@wdio/tauri-plugin");
  }

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
}

void bootstrap();
