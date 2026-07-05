import { describe, expect, it } from "vitest";
import { highlightCodeLine, languageIdForPath } from "./syntax";

describe("languageIdForPath", () => {
  it("detects common source file extensions", () => {
    expect(languageIdForPath("src/App.tsx")).toBe("tsx");
    expect(languageIdForPath("src/main.rs")).toBe("rust");
    expect(languageIdForPath("package.json")).toBe("json");
    expect(languageIdForPath("docs/HANDOFF.md")).toBe("markdown");
    expect(languageIdForPath("styles/index.css")).toBe("css");
    expect(languageIdForPath("scripts/release.sh")).toBe("shell");
  });

  it("detects special and compound file names", () => {
    expect(languageIdForPath("Dockerfile")).toBe("dockerfile");
    expect(languageIdForPath(".github/workflows/ci.yml")).toBe("yaml");
    expect(languageIdForPath("Cargo.toml")).toBe("toml");
  });

  it("returns null for unknown text files", () => {
    expect(languageIdForPath("notes.unknown")).toBeNull();
  });
});

describe("highlightCodeLine", () => {
  it("highlights TypeScript keywords, functions, strings, and numbers", () => {
    const tokens = highlightCodeLine('const total = add("x", 42);', "ts");
    expect(tokens).toContainEqual({ text: "const", kind: "keyword" });
    expect(tokens).toContainEqual({ text: "add", kind: "function" });
    expect(tokens).toContainEqual({ text: '"x"', kind: "string" });
    expect(tokens).toContainEqual({ text: "42", kind: "number" });
  });

  it("highlights Rust comments, keywords, types, and strings", () => {
    expect(highlightCodeLine("// hello", "rust")).toEqual([{ text: "// hello", kind: "comment" }]);
    const tokens = highlightCodeLine('pub fn main() -> Result<()> { println!("ok"); }', "rust");
    expect(tokens).toContainEqual({ text: "pub", kind: "keyword" });
    expect(tokens).toContainEqual({ text: "fn", kind: "keyword" });
    expect(tokens).toContainEqual({ text: "Result", kind: "type" });
    expect(tokens).toContainEqual({ text: '"ok"', kind: "string" });
  });

  it("highlights JSON object keys and literals", () => {
    const tokens = highlightCodeLine('"version": 2, "enabled": true', "json");
    expect(tokens).toContainEqual({ text: '"version"', kind: "property" });
    expect(tokens).toContainEqual({ text: "2", kind: "number" });
    expect(tokens).toContainEqual({ text: "true", kind: "keyword" });
  });

  it("keeps token text as raw text for React to escape", () => {
    const tokens = highlightCodeLine('const x = "<script>";', "ts");
    expect(tokens).toContainEqual({ text: '"<script>"', kind: "string" });
  });
});
