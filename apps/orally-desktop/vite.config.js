import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const uiPath = fileURLToPath(new URL("./ui", import.meta.url));

export default defineConfig({
  root: uiPath,
  base: "./",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": uiPath },
  },
  server: {
    host: "localhost",
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: fileURLToPath(new URL("./dist", import.meta.url)),
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./ui/index.html", import.meta.url)),
        overlay: fileURLToPath(new URL("./ui/overlay.html", import.meta.url)),
      },
    },
  },
});
