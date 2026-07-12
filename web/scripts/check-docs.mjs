import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const historical = new Set(["docs/IMPLEMENTATION_PLAN.md", "docs/PROGRESS.md", "docs/MILESTONES.md"]);
const markdown = [];
function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (["node_modules", ".git", "target", "dist"].includes(name)) continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (name.endsWith(".md")) markdown.push(path);
  }
}
walk(root);

const errors = [];
const normalize = (path) => relative(root, path).split(sep).join("/");
for (const file of markdown) {
  if (historical.has(normalize(file))) continue;
  const body = readFileSync(file, "utf8");
  for (const match of body.matchAll(/!?(?:\[[^\]]*\])\(([^)]+)\)/g)) {
    let target = match[1].trim().replace(/^<|>$/g, "").split("#")[0].split("?")[0];
    if (!target || /^(?:https?:|mailto:)/i.test(target)) continue;
    try { target = decodeURIComponent(target); } catch {}
    const destination = resolve(dirname(file), target);
    if (!existsSync(destination)) errors.push(`${normalize(file)}: missing ${match[1]}`);
  }
}

const pkg = JSON.parse(readFileSync(join(root, "web/package.json"), "utf8"));
const webReadme = readFileSync(join(root, "web/README.md"), "utf8");
for (const script of Object.keys(pkg.scripts)) {
  if (!webReadme.includes(`\`${script}\``)) errors.push(`web/README.md: undocumented package script ${script}`);
}
const readme = readFileSync(join(root, "README.md"), "utf8");
for (const section of ["Status", "Quick start", "Production container", "Architecture", "Measured results", "Security model and limitations", "Repository map", "Documentation", "Contributing", "License"]) {
  if (!readme.includes(`## ${section}`)) errors.push(`README.md: missing section ${section}`);
}
if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`docs: checked ${markdown.length - historical.size} Markdown files and ${Object.keys(pkg.scripts).length} package scripts`);
