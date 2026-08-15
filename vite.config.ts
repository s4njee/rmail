import path from "node:path";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

const calendarUiDir = path.resolve(__dirname, "rcalendar/packages/calendar-ui");

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [solid()],
  resolve: {
    alias: {
      "@rcalendar/ui/tokens.css": path.resolve(
        calendarUiDir,
        "src/tokens/tokens.css",
      ),
      "@rcalendar/ui": path.resolve(calendarUiDir, "src/index.ts"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1422,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1423,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
