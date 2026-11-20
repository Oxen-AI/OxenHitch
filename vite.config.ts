import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1437, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/core/**", "**/target/**"] } },
  build: { target: "safari15" },
  test: { environment: "jsdom", setupFiles: ["src/test-setup.ts"], css: false },
});
