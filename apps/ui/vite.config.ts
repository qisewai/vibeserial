import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    // 强制使用 IPv4，避免部分环境在 ::1 上绑定失败导致白屏。
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
  },
});
