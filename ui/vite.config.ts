import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// The app embeds `dist/` at build time. There is no dev server, so the CSP stays the same
// in development and in release builds.
export default defineConfig({
  plugins: [svelte()],
  // The bundled Noto fonts (OFL) are also the overlay text fonts.
  publicDir: "../fonts",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
