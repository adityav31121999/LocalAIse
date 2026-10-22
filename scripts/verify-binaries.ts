/**
 * Verifies the llama.cpp runtime bundled in resources/llama against the
 * SHA-256 checksums recorded by scripts/fetch-llama-server.ts in
 * resources/llama/checksums.json.
 *
 * Since llama.cpp v0.7.0 the bundle is not just one exe: it also carries
 * ggml-base.dll and the ggml-cpu-<variant>.dll dispatch tiers next to it,
 * so every file that has a recorded checksum is verified. A bundled
 * llama-server executable with no recorded checksum fails too, which
 * usually means a stray file was dropped into the bundle directory.
 *
 * Run with `npx tsx scripts/verify-binaries.ts [--update]`.
 *
 * --update records the current checksums (only meaningful right after a
 * deliberate rebuild); without it the script fails on any mismatch.
 */

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const LLAMA_DIR = join(ROOT, "resources", "llama");
const CHECKSUMS_PATH = join(LLAMA_DIR, "checksums.json");

function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      return walk(path);
    }
    // The checksum ledger itself, and empty-folder placeholders, are not
    // runtime files.
    return name === "checksums.json" || statSync(path).size === 0 ? [] : [path];
  });
}

function main(): void {
  const update = process.argv.includes("--update");
  let recorded: Record<string, string> = {};
  if (existsSync(CHECKSUMS_PATH)) {
    recorded = JSON.parse(readFileSync(CHECKSUMS_PATH, "utf8"));
  }

  const bundled = walk(LLAMA_DIR);

  if (bundled.length === 0) {
    console.log("No llama-server binaries bundled (resources/llama is empty).");
    process.exit(0);
  }

  if (update) {
    for (const path of bundled) {
      const key = path.slice(LLAMA_DIR.length + 1).split(/[\\/]/).join("/");
      recorded[key] = sha256(path);
      console.log(`recorded ${key} = ${recorded[key]}`);
    }
    writeFileSync(CHECKSUMS_PATH, JSON.stringify(recorded, null, 2) + "\n");
    return;
  }

  let failures = 0;
  for (const path of bundled) {
    const key = path.slice(LLAMA_DIR.length + 1).split(/[\\/]/).join("/");
    const expected = recorded[key];
    if (!expected) {
      console.error(
        `✘ ${key}: no checksum recorded; rebuild with fetch-llama-server.ts or add --update`,
      );
      failures++;
    } else if (sha256(path) !== expected) {
      console.error(`✘ ${key}: checksum mismatch`);
      failures++;
    } else {
      console.log(`✔ ${key}`);
    }
  }

  if (failures > 0) process.exit(1);
  console.log("All bundled llama.cpp files match their recorded checksums.");
}

main();
