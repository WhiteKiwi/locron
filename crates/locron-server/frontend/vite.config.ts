import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { cpSync } from "node:fs";
import { resolve } from "node:path";

export default defineConfig({
  base: "/",
  publicDir: "public",
  plugins: [react(), { name: "locron-font-license", closeBundle() { cpSync(resolve(import.meta.dirname, "../assets/fonts"), resolve(import.meta.dirname, "dist/fonts"), { recursive: true }); } }],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    assetsInlineLimit: 0,
    rollupOptions: { output: { entryFileNames: "assets/app-[hash].js", chunkFileNames: "assets/chunk-[hash].js", assetFileNames: "assets/[name]-[hash][extname]" } }
  },
  test: { environment: "jsdom", globals: true }
});
