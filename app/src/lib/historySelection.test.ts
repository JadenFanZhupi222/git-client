import { beforeEach, describe, expect, it } from "vitest";
import { readHistorySelection, writeHistorySelection } from "./historySelection";

describe("history selection persistence", () => {
  beforeEach(() => localStorage.clear());

  it("round-trips the selected commit and file per repository", () => {
    writeHistorySelection("C:\\repo", { commitId: "abc123", file: "src/App.tsx" });

    expect(readHistorySelection("C:\\repo")).toEqual({
      commitId: "abc123",
      file: "src/App.tsx",
    });
    expect(readHistorySelection("C:\\other")).toBeNull();
  });

  it("drops malformed stored state", () => {
    localStorage.setItem("history.selection.v1:C:\\repo", "not-json");

    expect(readHistorySelection("C:\\repo")).toBeNull();
    expect(localStorage.getItem("history.selection.v1:C:\\repo")).toBeNull();
  });
});
