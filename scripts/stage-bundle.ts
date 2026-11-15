/**
 * Stages the CPU base binaries and supported GPU backends into
 * src-tauri/bundle-llama/<platform>/ for packaging.
 */

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

// CUDA installers bring proprietary/runtime DLLs and remain an in-app install.
// Vulkan ships without those extra driver files and may be bundled.
const FORBIDDEN_PATTERN = /(cuda|opencl|cudart|cublas)/i;

function currentPlatform(): string {
  if (process.platform === "win32") return "windows-x64";
  if (process.platform === "linux") return "linux-x64";
  throw new Error(`Unsupported platform: ${process.platform}`);
}

function stageBundle(platform: string): void {
  const sourceCandidates = [
    join(ROOT, "resources", "llama", platform),
    join(ROOT, "resources", "llama", `${platform}-cpu`),
    join(ROOT, "src-tauri", "bundle-llama", platform),
  ];

  const sourceDir = sourceCandidates.find((dir) => existsSync(dir) && readdirSync(dir).some(f => !f.startsWith(".")));
  if (!sourceDir) {
    console.warn(`[stage-bundle] No populated source directory found for ${platform}. Checked: ${sourceCandidates.join(", ")}`);
    return;
  }

  const targetDir = join(ROOT, "src-tauri", "bundle-llama", platform);
  rmSync(targetDir, { recursive: true, force: true });
  mkdirSync(targetDir, { recursive: true });

  console.log(`[stage-bundle] Staging CPU base from ${sourceDir} to ${targetDir}...`);
  let count = 0;

  for (const file of readdirSync(sourceDir)) {
    if (file.startsWith(".")) continue;
    if (FORBIDDEN_PATTERN.test(file)) {
      console.log(`[stage-bundle] Skipping unsupported or proprietary GPU file: ${file}`);
      continue;
    }

    const src = join(sourceDir, file);
    const dst = join(targetDir, file);
    if (statSync(src).isFile()) {
      copyFileSync(src, dst);
      count++;
    } else if (statSync(src).isDirectory()) {
      console.warn(`[stage-bundle] Skipping directory: ${file}`);
    }
  }

  console.log(`[stage-bundle] Successfully staged ${count} CPU files for ${platform}.`);
}

const targetPlatform = process.argv[2] || currentPlatform();
stageBundle(targetPlatform);
