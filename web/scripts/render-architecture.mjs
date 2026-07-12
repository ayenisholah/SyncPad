import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { chromium } from "playwright";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const source = readFileSync(join(root, "docs/architecture.md"), "utf8");
const diagram = source.match(/```mermaid\s*([\s\S]*?)```/)?.[1];
if (!diagram) throw new Error("docs/architecture.md has no Mermaid block");
const input = join(root, "docs/architecture.mmd");
writeFileSync(input, diagram.trim() + "\n");
const puppeteerConfig = join(root, "web/node_modules/.cache/syncpad-puppeteer.json");
writeFileSync(puppeteerConfig, JSON.stringify({ executablePath: chromium.executablePath(), args: ["--no-sandbox"] }));
const command = process.platform === "win32" ? "npx.cmd" : "npx";
const result = spawnSync(command, ["mmdc", "-p", puppeteerConfig, "-i", input, "-o", join(root, "docs/architecture.png"), "-b", "transparent", "-w", "1600"], {
  cwd: join(root, "web"), stdio: "inherit", env: process.env, shell: process.platform === "win32",
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
