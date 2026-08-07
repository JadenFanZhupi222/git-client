import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CommitFileList } from "./CommitFileList";
import type { FileChangeDto } from "../ipc";

const files: FileChangeDto[] = [
  { path: "src/App.tsx", status: "modified", additions: 4, deletions: 1 },
  { path: "README.md", status: "added", additions: 12, deletions: 0 },
];

describe("CommitFileList", () => {
  it("exposes selectable file options and keyboard activation", () => {
    const onSelect = vi.fn();
    render(<CommitFileList files={files} selected="src/App.tsx" onSelect={onSelect} />);

    expect(screen.getByRole("listbox")).toBeInTheDocument();
    const options = screen.getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[1]).toHaveAttribute("aria-selected", "false");

    fireEvent.keyDown(options[1], { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("README.md");
  });

  it("keeps file history reachable for the selected row", () => {
    const onFileHistory = vi.fn();
    render(
      <CommitFileList
        files={files}
        selected="src/App.tsx"
        onSelect={vi.fn()}
        onFileHistory={onFileHistory}
      />,
    );

    const history = screen.getByRole("button", { name: /src\/App\.tsx/ });
    expect(history).toHaveAttribute("tabindex", "0");
    fireEvent.click(history);
    expect(onFileHistory).toHaveBeenCalledWith("src/App.tsx");
  });
});
