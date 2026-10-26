import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

// Tauri drives the dev server on a fixed port and expects the webview to own it.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Cargo rebuilds and locks the DLLs under target/ while tauri dev runs;
    // the Rust side has its own watcher, so vite must not watch it at all.
    // The same holds for resources/llama: in dev that folder IS the runtime
    // directory, so llama-server locks its DLLs while it runs, and the GPU
    // installer writes new ones there — vite's watcher dies with EBUSY on
    // Windows either way.
    watch: {
      ignored: ["**/src-tauri/**", "**/resources/**", "**/vendor/**", "**/build/**"],
    },
  },
  build: {
    target: "es2021",
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-react": ["react", "react-dom", "react-dom/client"],
          "vendor-zustand": ["zustand"],
          "vendor-pdf": ["pdfjs-dist"],
          "vendor-lucide": ["lucide-react"],
        },
      },
    },
  },
});