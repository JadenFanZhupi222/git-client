import { StateField, type Extension, type Range, type Text } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { highlightCodeLine, type SyntaxLang, type SyntaxKind } from "./syntax";

export function syntaxExtensionForLang(lang: SyntaxLang | null): Extension[] {
  if (!lang) return [];
  return [
    StateField.define<DecorationSet>({
      create(state) {
        return Decoration.set(syntaxDecorationRanges(state.doc, lang), true);
      },
      update(value, tr) {
        if (!tr.docChanged) return value.map(tr.changes);
        return Decoration.set(syntaxDecorationRanges(tr.state.doc, lang), true);
      },
      provide: (field) => EditorView.decorations.from(field),
    }),
  ];
}

export function syntaxDecorationRanges(doc: Text, lang: SyntaxLang): Range<Decoration>[] {
  const ranges: Range<Decoration>[] = [];
  for (let lineNo = 1; lineNo <= doc.lines; lineNo++) {
    const line = doc.line(lineNo);
    let offset = 0;
    for (const token of highlightCodeLine(line.text, lang)) {
      const from = line.from + offset;
      const to = from + token.text.length;
      if (token.kind !== "plain" && to > from) {
        ranges.push(Decoration.mark({ class: syntaxClass(token.kind) }).range(from, to));
      }
      offset += token.text.length;
    }
  }
  return ranges;
}

function syntaxClass(kind: SyntaxKind): string {
  return `syn-${kind}`;
}
