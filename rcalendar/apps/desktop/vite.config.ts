import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Resolve @rcalendar/ui straight to source so the desktop app always builds
// against the latest calendar-ui without a separate build step.
const calendarUiSrc = path.resolve(__dirname, "../../packages/calendar-ui/src/index.ts");

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      "@rcalendar/ui": calendarUiSrc,
    },
  },
  // Tauri expects a fixed dev port. 1421 (not the Tauri default 1420) so the
  // dev server doesn't collide with other projects that use 1420.
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1421,
    strictPort: true,
    allowedHosts: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
  },
});
