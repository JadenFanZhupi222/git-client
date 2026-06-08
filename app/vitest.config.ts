import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// 独立于 vite.config.ts(那份专供 tauri dev/build)。这里只配测试:
// jsdom 提供 DOM、globals 让 describe/it/expect 免 import、setup 引入 jest-dom 断言。
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
  },
});
