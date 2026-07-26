import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function findOversizedEntryChunks(manifest, sizeByFile, maxBytes) {
  return Object.values(manifest)
    .filter((chunk) => chunk.isEntry)
    .map((chunk) => ({
      file: chunk.file,
      size: sizeByFile[chunk.file] ?? 0,
    }))
    .filter((chunk) => chunk.size > maxBytes)
    .sort((a, b) => b.size - a.size);
}

export async function checkBundleSize(distDir, maxBytes) {
  const manifestPath = path.join(distDir, ".vite", "manifest.json");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const sizeEntries = await Promise.all(
    Object.values(manifest)
      .filter((chunk) => chunk.isEntry)
      .map(async (chunk) => [
        chunk.file,
        (await stat(path.join(distDir, chunk.file))).size,
      ]),
  );
  return findOversizedEntryChunks(
    manifest,
    Object.fromEntries(sizeEntries),
    maxBytes,
  );
}

async function main() {
  const distArg = process.argv[2] || "app/dist";
  const maxBytes = Number(process.argv[3] || 500_000);
  if (!Number.isFinite(maxBytes) || maxBytes <= 0) {
    throw new Error(`Invalid bundle budget: ${process.argv[3]}`);
  }
  const distDir = path.resolve(distArg);
  const oversized = await checkBundleSize(distDir, maxBytes);

  if (oversized.length) {
    console.error(`Initial JavaScript bundle budget exceeded (${maxBytes} bytes):`);
    for (const chunk of oversized) {
      console.error(`- ${chunk.file}: ${chunk.size} bytes`);
    }
    process.exitCode = 1;
    return;
  }
  console.log(`Initial JavaScript bundle budget passed (${maxBytes} bytes).`);
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main();
}
