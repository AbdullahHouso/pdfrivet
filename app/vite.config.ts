import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// Set by `tauri dev` when running on a phone or another device on the network.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  // Keep Rust compiler errors visible in the terminal.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
    // Translations live in ../locales, outside the app folder.
    fs: { allow: [".."] },
  },
  build: {
    target: "es2023",
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
