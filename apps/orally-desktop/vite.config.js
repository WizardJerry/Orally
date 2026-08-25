import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  server: {
    host: "localhost",
    port: 1420,
    strictPort: true,
  },
});
