import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { syntaxExtensionForLang } from "./cmSyntax";

describe("syntaxExtensionForLang", () => {
  it("adds token decorations to CodeMirror documents", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    const state = EditorState.create({
      doc: 'const total = add("x", 42);',
      extensions: syntaxExtensionForLang("ts"),
    });
    const view = new EditorView({ state, parent });

    expect(parent.querySelector(".syn-keyword")?.textContent).toBe("const");
    expect(parent.querySelector(".syn-function")?.textContent).toBe("add");
    expect(parent.querySelector(".syn-string")?.textContent).toBe('"x"');
    expect(parent.querySelector(".syn-number")?.textContent).toBe("42");
    view.destroy();
    parent.remove();
  });
});
