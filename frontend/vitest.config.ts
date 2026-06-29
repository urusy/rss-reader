import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
    conditions: ["development", "browser"], // Solid のテスト時条件（公式推奨）
  },
  test: {
    environment: "jsdom",
  },
});
