/** Stage a pinned, checksum-verified prebuilt Vulkan backend without the SDK. */
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const platform = process.argv[2] ?? (process.platform === "win32" ? "windows-x64" : "linux-x64");
if (platform !== "windows-x64" && platform !== "linux-x64") {
  throw new Error(`Unsupported platform: ${platform}`);
}

const executable = platform === "windows-x64" ? "llama-server.exe" : "llama-server";
const backendFile = platform === "windows-x64" ? "ggml-vulkan.dll" : "libggml-vulkan.so";
const cpuDir = join(ROOT, "resources", "llama", platform);
const bundleDir = join(ROOT, "src-tauri", "bundle-llama", platform);
const manifest = JSON.parse(readFileSync(join(ROOT, "src-tauri", "backends.json"), "utf8")) as {
  tag: string;
  entries: { platform: string; backend: string; files: { url: string; sha256: string }[] }[];
};
const entry = manifest.entries.find((item) => item.platform === platform && item.backend === "vulkan");
if (!entry) throw new Error(`No prebuilt Vulkan archive configured for ${platform}`);
if (!existsSync(join(cpuDir, executable))) {
  throw new Error(`Build the CPU llama-server first; missing ${join(cpuDir, executable)}`);
}

const tempRoot = join(ROOT, "vendor", `vulkan-bundle-${platform}`);
mkdirSync(tempRoot, { recursive: true });
mkdirSync(bundleDir, { recursive: true });
const walk = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap((item) => {
  const path = join(dir, item.name);
  return item.isDirectory() ? walk(path) : [path];
});

for (const [index, file] of entry.files.entries()) {
  const response = await fetch(file.url);
  if (!response.ok) throw new Error(`Vulkan archive download failed (${response.status}): ${file.url}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== file.sha256) throw new Error(`SHA-256 mismatch for ${file.url}`);

  const archive = join(tempRoot, platform === "windows-x64" ? `backend-${index}.zip` : `backend-${index}.tar.gz`);
  const extractDir = join(tempRoot, `extract-${index}`);
  writeFileSync(archive, bytes);
  rmSync(extractDir, { recursive: true, force: true });
  mkdirSync(extractDir, { recursive: true });
  if (platform === "windows-x64") {
    execFileSync("powershell.exe", [
      "-NoProfile", "-Command",
      `Expand-Archive -LiteralPath '${archive.replaceAll("'", "''")}' -DestinationPath '${extractDir.replaceAll("'", "''")}' -Force`,
    ], { stdio: "inherit" });
  } else {
    execFileSync("tar", ["-xzf", archive, "-C", extractDir], { stdio: "inherit" });
  }
  const source = walk(extractDir).find((path) => path.split(/[\\/]/).pop() === backendFile);
  if (!source) throw new Error(`${backendFile} not found in ${file.url}`);
  for (const target of [join(cpuDir, backendFile), join(bundleDir, backendFile)]) copyFileSync(source, target);
}

if (platform === "linux-x64") {
  const { chmodSync } = await import("node:fs");
  chmodSync(join(bundleDir, executable), 0o755);
}

const checksumFile = join(ROOT, "resources", "llama", "checksums.json");
const checksums = existsSync(checksumFile)
  ? JSON.parse(readFileSync(checksumFile, "utf8")) as Record<string, string>
  : {};
for (const dir of [cpuDir, bundleDir]) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isFile()) {
      checksums[`${platform}/${name}`] = createHash("sha256").update(readFileSync(path)).digest("hex");
    }
  }
}
writeFileSync(checksumFile, `${JSON.stringify(checksums, null, 2)}\n`);
rmSync(tempRoot, { recursive: true, force: true });
console.log(`Staged prebuilt Vulkan backend for ${platform} from llama.cpp ${manifest.tag}.`);
