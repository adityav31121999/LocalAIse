import fs from "node:fs";
import path from "node:path";
import JSZip from "jszip";

const ROOT_DIR = path.resolve(".");
const DEFAULT_OUTPUT = path.join(ROOT_DIR, "LocalAIse-source.zip");

const EXCLUDE_PATTERNS = [
  /[\\/]node_modules([\\/]|$)/,
  /[\\/]src-tauri[\\/]target([\\/]|$)/,
  /[\\/]dist([\\/]|$)/,
  /[\\/]\.git([\\/]|$)/,
  /[\\/]vendor[\\/]llama\.cpp([\\/]|$)/,
];

function shouldInclude(filePath: string, zipOutput: string): boolean {
  if (path.resolve(filePath) === path.resolve(zipOutput)) {
    return false;
  }
  for (const pattern of EXCLUDE_PATTERNS) {
    if (pattern.test(filePath)) {
      return false;
    }
  }
  return true;
}

function collectFiles(dir: string, zipOutput: string): string[] {
  const result: string[] = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (!shouldInclude(fullPath, zipOutput)) {
      continue;
    }
    if (entry.isDirectory()) {
      result.push(...collectFiles(fullPath, zipOutput));
    } else if (entry.isFile()) {
      result.push(fullPath);
    }
  }

  return result;
}

async function main() {
  const customOut = process.argv[2];
  const zipPath = customOut ? path.resolve(customOut) : DEFAULT_OUTPUT;

  console.log(`Scanning source files in: ${ROOT_DIR}`);
  const files = collectFiles(ROOT_DIR, zipPath);
  console.log(`Found ${files.length} files to archive.`);

  const zip = new JSZip();

  for (const file of files) {
    const rel = path.relative(ROOT_DIR, file).replace(/\\/g, "/");
    const entryName = `LocalAIse-main/${rel}`;
    const data = fs.readFileSync(file);
    zip.file(entryName, data);
  }

  console.log(`Compressing into ${zipPath} ...`);
  const content = await zip.generateAsync({
    type: "nodebuffer",
    compression: "DEFLATE",
    compressionOptions: { level: 6 },
  });

  fs.writeFileSync(zipPath, content);
  const sizeMb = (fs.statSync(zipPath).size / (1024 * 1024)).toFixed(2);
  console.log(`✓ Successfully created zip archive: ${zipPath} (${sizeMb} MB)`);
}

main().catch((err) => {
  console.error("Failed to create zip archive:", err);
  process.exit(1);
});
