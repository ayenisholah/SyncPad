import { test, expect, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import sharp from "sharp";
import gifenc from "gifenc";

const { GIFEncoder, applyPalette, quantize } = gifenc;

const output = resolve("../docs/assets");

async function waitForEditor(page: Page) {
  await page.locator(".monaco-editor").waitFor({ state: "visible" });
  await expect(page.locator(".editor-status")).toContainText("connected");
}

async function content(page: Page): Promise<string> {
  return page.evaluate(() => (window as any).monaco.editor.getModels()[0].getValue());
}

test("capture deterministic two-browser convergence demo", async ({ browser }) => {
  await mkdir(output, { recursive: true });
  const a = await browser.newPage({ viewport: { width: 720, height: 640 } });
  const response = await a.request.post("/api/docs");
  const { docId } = await response.json() as { docId: string };
  const b = await browser.newPage({ viewport: { width: 720, height: 640 } });
  await Promise.all([a.goto(`/d/${docId}`), b.goto(`/d/${docId}`)]);
  await Promise.all([waitForEditor(a), waitForEditor(b)]);
  await b.locator("select[aria-label=Language]").selectOption("typescript");
  await expect(a.locator("select[aria-label=Language]")).toHaveValue("typescript");

  const frames: Buffer[] = [];
  async function capture() {
    const [left, right] = await Promise.all([a.screenshot(), b.screenshot()]);
    const joined = await sharp({ create: { width: 1448, height: 640, channels: 4, background: "#09070f" } })
      .composite([{ input: left, left: 0, top: 0 }, { input: right, left: 728, top: 0 }])
      .png().toBuffer();
    frames.push(joined);
  }

  await capture();
  await Promise.all([a.locator(".monaco-editor").click(), b.locator(".monaco-editor").click()]);
  await Promise.all([a.keyboard.type("const shared = ", { delay: 35 }), b.keyboard.type("// together\n", { delay: 35 })]);
  await capture();
  await Promise.all([a.keyboard.type("42;", { delay: 45 }), b.keyboard.type("export ", { delay: 45 })]);
  await expect.poll(async () => [await content(a), await content(b)]).toEqual([await content(a), await content(a)]);
  await capture();
  await writeFile(resolve(output, "demo.png"), frames.at(-1)!);

  const gif = GIFEncoder();
  for (const frame of frames) {
    const { data, info } = await sharp(frame).resize({ width: 900 }).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
    const palette = quantize(data, 128);
    gif.writeFrame(applyPalette(data, palette), info.width, info.height, { palette, delay: 900 });
  }
  // Hold the converged result for a final beat.
  const { data, info } = await sharp(frames.at(-1)!).resize({ width: 900 }).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const palette = quantize(data, 128);
  gif.writeFrame(applyPalette(data, palette), info.width, info.height, { palette, delay: 1800 });
  gif.finish();
  await writeFile(resolve(output, "demo.gif"), Buffer.from(gif.bytes()));
});
