import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// https://vitejs.dev/config/
export default defineConfig(async () => {
  const isDev = process.env.NODE_ENV === 'development';
  
  return {
    plugins: [vue()],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    // prevent vite from obscuring rust errors
    clearScreen: false,
    // tauri expects a fixed port, fail if that port is not available
    server: {
      port: 1420,
      strictPort: true,
    },
    // to make use of `TAURI_DEBUG` and other env variables
    // https://tauri.studio/v1/api/config#buildconfig.beforedevcommand
    envPrefix: ["VITE_", "TAURI_"],
    build: {
      // Tauri supports es2021
      target: process.env.TAURI_PLATFORM === "windows" ? "chrome105" : "safari13",
      // don't minify in dev mode
      minify: !isDev ? "esbuild" : false,
      // produce sourcemaps for debug builds
      sourcemap: isDev,
      // Control console output
      terserOptions: {
        compress: {
          // Only keep console.error in production
          drop_console: !isDev,
          pure_funcs: !isDev ? ['console.log', 'console.debug', 'console.info', 'console.warn'] : [],
        },
      },
    },
  };
});
