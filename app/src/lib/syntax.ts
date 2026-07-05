export type SyntaxLang =
  | "ts"
  | "tsx"
  | "js"
  | "jsx"
  | "rust"
  | "json"
  | "markdown"
  | "css"
  | "html"
  | "python"
  | "yaml"
  | "toml"
  | "shell"
  | "dockerfile";

export type SyntaxKind = "plain" | "keyword" | "string" | "number" | "comment" | "type" | "function" | "property" | "operator";

export type SyntaxToken = {
  text: string;
  kind: SyntaxKind;
};

const EXT_LANG: Record<string, SyntaxLang> = {
  ts: "ts",
  tsx: "tsx",
  js: "js",
  jsx: "jsx",
  mjs: "js",
  cjs: "js",
  rs: "rust",
  json: "json",
  md: "markdown",
  markdown: "markdown",
  css: "css",
  scss: "css",
  less: "css",
  html: "html",
  htm: "html",
  py: "python",
  yml: "yaml",
  yaml: "yaml",
  toml: "toml",
  sh: "shell",
  bash: "shell",
  zsh: "shell",
  ps1: "shell",
};

const SPECIAL_NAMES: Record<string, SyntaxLang> = {
  dockerfile: "dockerfile",
  "dockerfile.dev": "dockerfile",
  makefile: "shell",
};

const JS_KEYWORDS = new Set([
  "as",
  "async",
  "await",
  "break",
  "case",
  "catch",
  "class",
  "const",
  "continue",
  "default",
  "else",
  "export",
  "extends",
  "false",
  "finally",
  "for",
  "from",
  "function",
  "if",
  "import",
  "in",
  "interface",
  "let",
  "new",
  "null",
  "of",
  "return",
  "switch",
  "throw",
  "true",
  "try",
  "type",
  "undefined",
  "while",
]);

const RUST_KEYWORDS = new Set([
  "as",
  "async",
  "await",
  "break",
  "const",
  "continue",
  "crate",
  "else",
  "enum",
  "false",
  "fn",
  "for",
  "if",
  "impl",
  "in",
  "let",
  "match",
  "mod",
  "move",
  "mut",
  "pub",
  "ref",
  "return",
  "self",
  "static",
  "struct",
  "super",
  "trait",
  "true",
  "type",
  "use",
  "where",
  "while",
]);

const PY_KEYWORDS = new Set(["and", "as", "async", "await", "class", "def", "elif", "else", "False", "for", "from", "if", "import", "in", "None", "not", "or", "return", "True", "while", "with"]);
const SHELL_KEYWORDS = new Set(["case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in", "then", "while"]);
const DOCKER_KEYWORDS = new Set(["add", "arg", "cmd", "copy", "entrypoint", "env", "expose", "from", "label", "run", "user", "volume", "workdir"]);

export function languageIdForPath(path: string): SyntaxLang | null {
  const normalized = path.replace(/\\/g, "/");
  const name = normalized.split("/").pop()?.toLowerCase() ?? "";
  if (!name) return null;
  if (SPECIAL_NAMES[name]) return SPECIAL_NAMES[name];
  const ext = name.includes(".") ? name.split(".").pop() ?? "" : "";
  return EXT_LANG[ext] ?? null;
}

export function highlightCodeLine(text: string, lang: SyntaxLang | null): SyntaxToken[] {
  if (!lang || text.length === 0) return [{ text, kind: "plain" }];
  if (lang === "json") return mergePlain(tokenize(text, lang, new Set(["true", "false", "null"])));
  if (lang === "rust") return mergePlain(tokenize(text, lang, RUST_KEYWORDS));
  if (lang === "python") return mergePlain(tokenize(text, lang, PY_KEYWORDS));
  if (lang === "shell") return mergePlain(tokenize(text, lang, SHELL_KEYWORDS));
  if (lang === "dockerfile") return mergePlain(tokenize(text, lang, DOCKER_KEYWORDS));
  if (lang === "ts" || lang === "tsx" || lang === "js" || lang === "jsx") return mergePlain(tokenize(text, lang, JS_KEYWORDS));
  return mergePlain(tokenize(text, lang, new Set()));
}

function tokenize(text: string, lang: SyntaxLang, keywords: Set<string>): SyntaxToken[] {
  const out: SyntaxToken[] = [];
  let i = 0;

  const push = (end: number, kind: SyntaxKind) => {
    out.push({ text: text.slice(i, end), kind });
    i = end;
  };

  while (i < text.length) {
    if (startsComment(text, i, lang)) {
      push(text.length, "comment");
      continue;
    }

    const quote = text[i];
    if (quote === '"' || quote === "'" || quote === "`") {
      const end = readString(text, i, quote);
      const kind = lang === "json" && isJsonProperty(text, end) ? "property" : "string";
      push(end, kind);
      continue;
    }

    if (isDigit(text[i])) {
      const m = /^[0-9]+(?:\.[0-9]+)?/.exec(text.slice(i));
      if (m) {
        push(i + m[0].length, "number");
        continue;
      }
    }

    if (isIdentStart(text[i])) {
      const m = /^[A-Za-z_$][A-Za-z0-9_$-]*/.exec(text.slice(i));
      if (m) {
        const word = m[0];
        const end = i + word.length;
        const lower = word.toLowerCase();
        if (keywords.has(word) || keywords.has(lower)) push(end, "keyword");
        else if (isProperty(text, end, lang)) push(end, "property");
        else if (isFunction(text, end)) push(end, "function");
        else if (/^[A-Z]/.test(word)) push(end, "type");
        else push(end, "plain");
        continue;
      }
    }

    if (/^[{}()[\].,;:+\-*/%=!<>|&?@]+$/.test(text[i])) {
      push(i + 1, "operator");
      continue;
    }

    push(i + 1, "plain");
  }

  return out;
}

function startsComment(text: string, i: number, lang: SyntaxLang): boolean {
  if (text.startsWith("//", i)) return lang !== "json";
  if (text.startsWith("/*", i)) return lang === "css" || lang === "ts" || lang === "tsx" || lang === "js" || lang === "jsx" || lang === "rust";
  if (text[i] === "#") return lang === "shell" || lang === "python" || lang === "yaml" || lang === "toml" || lang === "dockerfile";
  if (text.startsWith("<!--", i)) return lang === "html" || lang === "markdown";
  return false;
}

function readString(text: string, start: number, quote: string): number {
  let escaped = false;
  for (let i = start + 1; i < text.length; i++) {
    if (escaped) {
      escaped = false;
      continue;
    }
    if (text[i] === "\\") {
      escaped = true;
      continue;
    }
    if (text[i] === quote) return i + 1;
  }
  return text.length;
}

function isJsonProperty(text: string, end: number): boolean {
  return /^\s*:/.test(text.slice(end));
}

function isProperty(text: string, end: number, lang: SyntaxLang): boolean {
  if (lang === "css" || lang === "yaml" || lang === "toml") return /^\s*:/.test(text.slice(end)) || /^\s*=/.test(text.slice(end));
  return false;
}

function isFunction(text: string, end: number): boolean {
  return /^\s*\(/.test(text.slice(end));
}

function isDigit(ch: string): boolean {
  return ch >= "0" && ch <= "9";
}

function isIdentStart(ch: string): boolean {
  return /[A-Za-z_$]/.test(ch);
}

function mergePlain(tokens: SyntaxToken[]): SyntaxToken[] {
  const out: SyntaxToken[] = [];
  for (const token of tokens) {
    const prev = out[out.length - 1];
    if (prev && prev.kind === token.kind) prev.text += token.text;
    else out.push({ ...token });
  }
  return out;
}
