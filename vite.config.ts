import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
    fs: {
      allow: [".."],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_PLATFORM == "windows" ? "chrome105" : "safari13",
    // Minify for production
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // Sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
    // Better chunk splitting
    chunkSizeWarningLimit: 500,
    // Code splitting
    cssCodeSplit: true,
    // Optimization
    reportCompressedSize: true,
    // Assets inline threshold
    assetsInlineLimit: 4096,
    // Disable this for faster builds in dev
    emptyOutDir: true,
    // Manual chunks for better code splitting
    rollupOptions: {
      output: {
        manualChunks: {
          "vue-vendor": ["vue"],
          "virtual-scroller": ["vue-virtual-scroller"],
          "tauri-api": ["@tauri-apps/api"],
          "tauri-plugins": ["@tauri-apps/plugin-dialog", "@tauri-apps/plugin-opener"],
        },
      },
    },
  },
  // Optimize deps
  optimizeDeps: {
    include: [
      "vue",
      "vue-virtual-scroller",
      "@tauri-apps/api",
      "@tauri-apps/plugin-dialog",
      "@tauri-apps/plugin-opener",
    ],
    exclude: [],
  },
  // Resolve aliases
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
    },
  },
});