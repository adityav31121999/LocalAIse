import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

// Libraries that must NEVER appear in the CPU-only release bundle
const FORBIDDEN = [
  /^ggml-cuda\.dll$/i,
  /^ggml-vulkan\.dll$/i,
  /^ggml-opencl\.dll$/i,
  /^nvcuda\.dll$/i,
  /^libcuda\.so/i,
  /^cudart/i,
  /^cublas/i,
  /^libcudart/i,
];

function walk(dir: string): string[] {
  try {
    return readdirSync(dir).flatMap((name) => {
      const path = join(dir, name);
      return statSync(path).isDirectory() ? walk(path) : [path];
    });
  } catch {
    return [];
  }
}

const targetDir = process.argv[2] || "./dist";
const offenders = walk(targetDir).filter((p) =>
  FORBIDDEN.some((re) => re.test(p.split(/[\\/]/).pop() ?? ""))
);

if (offenders.length > 0) {
  console.error("❌ GPU libraries found in bundle:");
  for (const f of offenders) console.error("  " + f);
  process.exit(1);
} else {
  console.log("✅ Bundle verified: Zero-VRAM / CPU-only compliance passed.");
}
