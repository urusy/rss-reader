import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [solid(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    port: 5173,
    proxy: {
      // Dev: forward API calls to the Rust backend. changeOrigin は付けない —
      // Host を書き換えるとブラウザの Origin と食い違い、CSRF の Origin 検証
      // （POST 等で Origin と Host の一致を要求）が 403 を返してしまう。
      "/api": { target: "http://localhost:8080" },
    },
  },
});
