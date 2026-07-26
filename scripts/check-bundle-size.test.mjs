import assert from "node:assert/strict";
import test from "node:test";

import { findOversizedEntryChunks } from "./check-bundle-size.mjs";

test("accepts entry chunks at or below the budget", () => {
  const manifest = {
    "index.html": {
      file: "assets/index.js",
      isEntry: true,
    },
    "src/lazy.tsx": {
      file: "assets/lazy.js",
      isDynamicEntry: true,
    },
  };

  assert.deepEqual(
    findOversizedEntryChunks(
      manifest,
      { "assets/index.js": 500_000, "assets/lazy.js": 900_000 },
      500_000,
    ),
    [],
  );
});

test("reports an oversized initial entry chunk", () => {
  const manifest = {
    "index.html": {
      file: "assets/index.js",
      isEntry: true,
    },
  };

  assert.deepEqual(
    findOversizedEntryChunks(
      manifest,
      { "assets/index.js": 500_001 },
      500_000,
    ),
    [{ file: "assets/index.js", size: 500_001 }],
  );
});
