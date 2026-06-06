import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./index.css";
import App from "./App";
import { ToastProvider } from "./components/Toast";
import { applyTheme, getStoredTheme } from "./lib/theme";

// 渲染前先定主题,避免首屏从浅色「闪」到暗色
applyTheme(getStoredTheme());

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
        <App />
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
