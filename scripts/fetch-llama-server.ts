/**
 * Builds llama-server for localAIse and drops the output into a single flat
 * platform folder (`resources/llama/windows-x64` or `resources/llama/linux-x64`).
 *
 * All backends share the same folder. Installing a GPU backend adds its
 * `ggml-<backend>.dll` (and any runtime DLLs such as `cudart64_*.dll`)
 * next to the existing CPU files. Removing a backend means deleting those
 * files. `llama-server` discovers available backends by scanning for
 * `ggml-*.dll` at startup — no separate subdirectories are needed.
 *
 * Usage:
 *   node scripts/fetch-llama-server.ts [platform] [backend]
 *
 *   platform: windows-x64 | linux-x64    (default: current platform)
 *   backend:  cpu | vulkan | cuda | opencl  (default: cpu)
 *
 * Examples:
 *   npm run fetch-llama-server                       # CPU for the current platform
 *   npm run fetch-llama-server -- windows-x64 cuda   # CUDA backend for Windows
 *   npm run fetch-llama-server -- linux-x64 vulkan   # Vulkan for Linux
 *
 * For GPU backends, only the `ggml-<backend>.dll` and its runtime
 * dependencies are new. The core DLLs (`ggml.dll`, `llama.dll`, ...) in
 * the GPU release archive are identical to the CPU build at the same tag,
 * so overwriting them is safe.
 *
 * Requirements on PATH beyond the CPU build:
 *   - vulkan:  the Vulkan SDK (`glslc` compiles the shaders)
 *   - cuda:    the CUDA Toolkit (`nvcc`)
 *   - opencl:  an OpenCL SDK with OpenCL.lib on the CMake search path
 *              (e.g. `vcpkg install opencl`; pass it with LLAMA_CMAKE_EXTRA)
 *
 * Run with `npx tsx scripts/fetch-llama-server.ts`.
 *
 * llama.cpp build tag note:
 *   llama.cpp does NOT use semantic versioning. It uses nightly build tags
 *   of the form `b<number>` (e.g. `b6742`). The PINNED_TAG constant below
 *   pins the exact nightly tag. Update it deliberately and re-audit
 *   `runtime/args.rs` each time.
 */

import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

// Pinned llama.cpp nightly build tag. Bump deliberately and re-audit
// the flag surface runtime/args.rs relies on each time.
// Override locally with LLAMA_TAG (e.g. `LLAMA_TAG=b6000 npm run fetch-llama-server`).
const PINNED_TAG = process.env.LLAMA_TAG ?? "b11678";

const CHECKOUT = join(ROOT, "vendor", "llama.cpp");

/**
 * CMake flags shared by every backend. Since b5000+,
 * GGML_CPU_ALL_VARIANTS requires GGML_BACKEND_DL, which in turn requires
 * shared libraries: the exe ships with ggml-base.dll, one
 * ggml-cpu-<variant>.dll per x86-64 tier — and with a GPU backend enabled,
 * its ggml-<backend>.dll next to them, loaded from the exe's directory.
 */
const BASE_FLAGS = [
  "-DBUILD_SHARED_LIBS=ON",
  "-DGGML_BACKEND_DL=ON",
  "-DGGML_HIP=OFF",
  "-DGGML_SYCL=OFF",
  "-DLLAMA_CURL=OFF",
  "-DGGML_NATIVE=OFF",
  "-DGGML_CPU_ALL_VARIANTS=ON",
  "-DLLAMA_BUILD_TESTS=OFF",
  "-DLLAMA_BUILD_EXAMPLES=OFF",
  "-DGGML_CCACHE=ON",
  "-DCMAKE_CXX_FLAGS_RELEASE=/O2 /Ob2 /DNDEBUG /MP /wd4221 /wd4514 /wd4548",
  "-DCMAKE_C_FLAGS_RELEASE=/O2 /Ob2 /DNDEBUG /MP /wd4221 /wd4514 /wd4548",
  "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL",
];

type Backend = "cpu" | "vulkan" | "cuda" | "opencl";
type Platform = "windows-x64" | "linux-x64";

/** CMake flags to enable one GPU backend; all others are disabled. */
function gpuFlags(backend: Backend): string[] {
  const on = (name: string) => `-D${name}=ON`;
  const extra =
    process.env.LLAMA_CMAKE_EXTRA?.split(/\s+/).filter(Boolean) ?? [];
  return [
    "-DGGML_CUDA=OFF",
    "-DGGML_VULKAN=OFF",
    "-DGGML_OPENCL=OFF",
    "-DGGML_METAL=OFF",
    ...(backend === "vulkan" ? [on("GGML_VULKAN")] : []),
    ...(backend === "cuda" ? [on("GGML_CUDA")] : []),
    ...(backend === "opencl" ? [on("GGML_OPENCL")] : []),
    ...extra,
  ];
}

function buildDirFor(backend: Backend): string {
  return `build-${backend}`;
}

function currentPlatform(): Platform {
  if (process.platform === "win32") return "windows-x64";
  if (process.platform === "linux") return "linux-x64";
  throw new Error(`No platform defined for ${process.platform}`);
}

function run(command: string, cwd: string): void {
  console.log(`$ ${command}`);
  execSync(command, { cwd, stdio: "inherit" });
}

function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function main(): void {
  const args = process.argv.slice(2);
  const platform: Platform = (args[0] as Platform | undefined) ?? currentPlatform();
  const backend: Backend = (args[1] as Backend | undefined) ?? "cpu";

  const validPlatforms: Platform[] = ["windows-x64", "linux-x64"];
  const validBackends: Backend[] = ["cpu", "vulkan", "cuda", "opencl"];

  if (!validPlatforms.includes(platform)) {
    throw new Error(
      `Unknown platform "${platform}". Valid: ${validPlatforms.join(", ")}.`,
    );
  }
  if (!validBackends.includes(backend)) {
    throw new Error(
      `Unknown backend "${backend}". Valid: ${validBackends.join(", ")}.`,
    );
  }

  const CHECKSUMS = join(ROOT, "resources", "llama", "checksums.json");
  if (!existsSync(CHECKOUT)) {
    throw new Error(
      "vendor/llama.cpp is missing. Check out the pinned tag first:\n" +
        `git clone --depth 1 --branch ${PINNED_TAG} https://github.com/ggml-org/llama.cpp vendor/llama.cpp`,
    );
  }

  // All backends write into the single flat platform folder.
  const outDir = join(ROOT, "resources", "llama", platform);
  const buildDir = join(CHECKOUT, buildDirFor(backend));
  mkdirSync(outDir, { recursive: true });
  mkdirSync(buildDir, { recursive: true });

  const flags = [...BASE_FLAGS, ...gpuFlags(backend)];
  run(`cmake -B ${buildDirFor(backend)} ${flags.join(" ")}`, CHECKOUT);
  run(
    `cmake --build ${buildDirFor(backend)} --config Release --target llama-server -- /p:NoWarn=MSB8027`,
    CHECKOUT,
  );

  const exeName = platform === "windows-x64" ? "llama-server.exe" : "llama-server";
  const built = [
    join(buildDir, "bin", "Release", exeName),
    join(buildDir, "bin", exeName),
  ].find((p) => existsSync(p));
  if (!built) throw new Error("llama-server was not produced by the build");

  // GPU builds: copy only new/changed files (the GPU backend DLL + its
  // runtime dependencies). The core DLLs from the GPU archive are identical
  // to the CPU build at the same tag, so overwriting them is safe.
  // CPU build: copy everything, but don't delete GPU backend DLLs already
  // present (a developer might have the GPU backend installed alongside).
  const builtDir = built.replace(exeName, "").replace(/[/\\]$/, "");

  for (const file of readdirSync(builtDir)) {
    const src = join(builtDir, file);
    if (statSync(src).isFile()) {
      copyFileSync(src, join(outDir, file));
    }
  }
  if (!existsSync(join(outDir, exeName))) {
    throw new Error(`llama-server disappeared while copying from ${builtDir}`);
  }
  if (platform === "linux-x64") chmodSync(join(outDir, exeName), 0o755);

  // Also copy to Tauri bundle resource directory (src-tauri/bundle-llama/<platform>)
  const tauriBundleDir = join(ROOT, "src-tauri", "bundle-llama", platform);
  mkdirSync(tauriBundleDir, { recursive: true });
  for (const file of readdirSync(builtDir)) {
    const src = join(builtDir, file);
    if (statSync(src).isFile()) {
      copyFileSync(src, join(tauriBundleDir, file));
    }
  }
  if (platform === "linux-x64") chmodSync(join(tauriBundleDir, exeName), 0o755);

  // Update the checksum ledger for the files in the platform folder.
  // All files are recorded under the flat platform key.
  let checksums: Record<string, string> = {};
  try {
    checksums = JSON.parse(readFileSync(CHECKSUMS, "utf8"));
  } catch {
    checksums = {};
  }
  // Re-compute all files in the outDir (checksums for GPU DLLs not present
  // in a CPU build are simply absent, so no stale entries accumulate).
  for (const file of readdirSync(outDir)) {
    const path = join(outDir, file);
    if (statSync(path).isFile()) {
      checksums[`${platform}/${file}`] = sha256(path);
    }
  }
  writeFileSync(CHECKSUMS, JSON.stringify(checksums, null, 2) + "\n");

  console.log(`✔ ${platform} (${backend}): ${statty(join(outDir, exeName))}`);
  console.log(
    `  ${readdirSync(outDir).length} files total in ${outDir}`,
  );
  console.log(`  checksums updated in ${CHECKSUMS}`);

  function statty(path: string): string {
    const mb = (statSync(path).size / 1_048_576).toFixed(1);
    return `${path} (${mb} MB)`;
  }
}

main();
