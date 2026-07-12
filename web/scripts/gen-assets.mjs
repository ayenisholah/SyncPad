// Regenerate SyncPad's raster brand assets from the SVG sources (dev-only).
//
//   npm run gen:assets
//
// The generated PNG/ICO files are committed to web/public so the production
// build (and CI) never needs native image tooling (D-009). SVG is the source
// of truth; re-run this after editing web/assets/*.svg.
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";
import pngToIco from "png-to-ico";

const here = dirname(fileURLToPath(import.meta.url));
const assets = resolve(here, "../assets");
const out = resolve(here, "../public");

const iconSvg = await readFile(resolve(assets, "favicon.svg"));
const ogSvg = await readFile(resolve(assets, "og.svg"));

await mkdir(out, { recursive: true });

/** Rasterize an SVG buffer to a square PNG of the given size. */
const png = (svg, size) =>
  sharp(svg, { density: Math.max(72, Math.ceil((size / 512) * 384)) })
    .resize(size, size, { fit: "contain" })
    .png()
    .toBuffer();

// Favicon PNGs, apple-touch, and PWA manifest icons.
const sizes = {
  "favicon-16x16.png": 16,
  "favicon-32x32.png": 32,
  "apple-touch-icon.png": 180,
  "icon-192.png": 192,
  "icon-512.png": 512,
};
for (const [name, size] of Object.entries(sizes)) {
  await writeFile(resolve(out, name), await png(iconSvg, size));
}

// Multi-resolution favicon.ico for legacy clients.
const ico = await pngToIco([
  await png(iconSvg, 16),
  await png(iconSvg, 32),
  await png(iconSvg, 48),
]);
await writeFile(resolve(out, "favicon.ico"), ico);

// The SVG favicon is served as-is (modern browsers prefer it).
await writeFile(resolve(out, "favicon.svg"), iconSvg);

// 1200x630 Open Graph / Twitter card.
await writeFile(
  resolve(out, "og.png"),
  await sharp(ogSvg, { density: 144 }).resize(1200, 630).png().toBuffer(),
);

console.log("generated brand assets in web/public/");
