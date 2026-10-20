import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

/** The allowed licenses (ADR 0010), in order of preference. CI checks the same list for npm production packages. */
const ALLOWED = new Set([
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "MPL-2.0",
  "Unicode-3.0",
]);

/**
 * Writes `third-party-npm.txt` with the license of each npm package that has code in
 * the bundle. The build fails on a license that is not allowed. CI adds the file to
 * THIRD_PARTY_LICENSES.
 */
function npmNotices(): Plugin {
  return {
    name: "t-rustable-npm-notices",
    apply: "build",
    generateBundle(_options, bundle) {
      const packages = new Map<string, string>();
      for (const file of Object.values(bundle)) {
        if (file.type !== "chunk") continue;
        for (const id of file.moduleIds) {
          const path = id.replace(/\\/g, "/").split("?")[0]!;
          const at = path.lastIndexOf("/node_modules/");
          if (at < 0) continue;
          const parts = path.slice(at + 14).split("/");
          const name = parts[0]!.startsWith("@") ? `${parts[0]}/${parts[1]}` : parts[0]!;
          packages.set(name, path.slice(0, at + 14) + name);
        }
      }
      const notices: string[] = [];
      for (const [name, dir] of [...packages].sort(([a], [b]) => a.localeCompare(b))) {
        const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as {
          version: string;
          license?: string | { type: string };
        };
        const license = typeof meta.license === "object" ? meta.license.type : (meta.license ?? "");
        // For "A OR B", the choice that comes first in ALLOWED applies, so MIT if it is offered.
        const choices = license
          .replace(/[()]/g, "")
          .split(" OR ")
          .map((l) => l.trim());
        const used = [...ALLOWED].find((l) => choices.includes(l));
        if (!used) this.error(`${name} has the license "${license}", which is not allowed.`);
        // The text of the chosen license, for example LICENSE-MIT, else the general file.
        const files = readdirSync(dir).filter((f: string) => /^(licen[cs]e|copying)/i.test(f) && !/\.spdx$/i.test(f));
        const file =
          files.find((f) => f.toUpperCase().includes(used!.toUpperCase())) ??
          files.find((f) => /^(licen[cs]e|copying)(\.(md|txt))?$/i.test(f));
        if (!file) this.error(`${name} has no license file.`);
        const text = readFileSync(join(dir, file!), "utf8").trim();
        const note = used === license ? license : `${license}, used under ${used}`;
        notices.push(`${name} ${meta.version}\nLicense: ${note}\n\n${text}\n`);
      }
      this.emitFile({
        type: "asset",
        fileName: "third-party-npm.txt",
        source: `npm packages in the T-Rustable UI\n\n${notices.join("\n----------\n\n")}`,
      });
    },
  };
}

// The app embeds `dist/` at build time. There is no dev server, so the CSP stays the same
// in development and in release builds.
export default defineConfig({
  plugins: [svelte(), npmNotices()],
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
