import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DiffView } from "./DiffView";
import type { FileDiffDto } from "../ipc";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 20,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        key: index,
        start: index * 20,
      })),
    measureElement: vi.fn(),
  }),
}));

vi.mock("../ipc", () => ({
  readImage: vi.fn(),
}));

const tsDiff: FileDiffDto = {
  path: "src/App.tsx",
  is_binary: false,
  too_large: false,
  is_lfs_pointer: false,
  lfs_size: "0",
  is_image: false,
  old_image: null,
  new_image: null,
  hunks: [
    {
      header: "@@ -1,1 +1,1 @@",
      lines: [
        { kind: "context", old_lineno: 1, new_lineno: 1, content: 'const total = add("x", 42);' },
      ],
    },
  ],
};

describe("DiffView syntax highlighting", () => {
  afterEach(() => {
    localStorage.clear();
  });

  it("renders syntax token spans for known text files", async () => {
    const { container } = render(<DiffView diff={tsDiff} loading={false} hasFile repo="D:/repo" />);

    expect(await screen.findByText("const")).toBeInTheDocument();
    expect(container.querySelector(".syn-keyword")?.textContent).toBe("const");
    expect(container.querySelector(".syn-function")?.textContent).toBe("add");
    expect(container.querySelector(".syn-string")?.textContent).toBe('"x"');
    expect(container.querySelector(".syn-number")?.textContent).toBe("42");
  });
});
