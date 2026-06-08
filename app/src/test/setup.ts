// 每个测试文件运行前自动加载:挂上 @testing-library/jest-dom 的自定义断言
//(toBeInTheDocument / toHaveValue 等),并在每个用例后清理渲染的 DOM。
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});
