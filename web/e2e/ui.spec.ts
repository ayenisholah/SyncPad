import { test, expect, type Page } from "@playwright/test";

// Editor chrome (W2D3-2): a language change re-highlights every window, the
// latency readout is live, and the presence bar shows peers.

async function waitForEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().waitFor({ state: "visible" });
  await page.waitForFunction(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    return Boolean(monaco?.editor?.getModels?.().length);
  });
  await expect(page.locator(".editor-status")).toContainText("connected");
}

async function languageId(page: Page): Promise<string> {
  return page.evaluate(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    return monaco?.editor?.getModels?.()[0]?.getLanguageId() ?? "";
  });
}

async function createDoc(page: Page): Promise<string> {
  const response = await page.request.post("/api/docs");
  expect(response.ok()).toBeTruthy();
  return ((await response.json()) as { docId: string }).docId;
}

test("language change re-highlights both windows and latency reads live", async ({
  browser,
}) => {
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const a = await ctxA.newPage();
  const b = await ctxB.newPage();

  const docId = await createDoc(a);
  await a.goto(`/d/${docId}`);
  await b.goto(`/d/${docId}`);
  await waitForEditor(a);
  await waitForEditor(b);

  // Each window shows the other in its presence bar.
  await expect(a.locator(".presence .avatar")).toHaveCount(2); // "you" + one peer
  await expect(b.locator(".presence .avatar")).toHaveCount(2);

  // A picks JavaScript; both editors re-highlight.
  await a.locator(".lang-select").selectOption("javascript");
  await expect.poll(() => languageId(a), { timeout: 15_000 }).toBe("javascript");
  await expect.poll(() => languageId(b), { timeout: 15_000 }).toBe("javascript");

  // A types; B's status bar shows a numeric op→apply latency.
  await a.locator(".monaco-editor").first().click();
  await a.keyboard.type("const x = 1;", { delay: 15 });
  await expect
    .poll(async () => (await b.locator(".status-latency").textContent()) ?? "", {
      timeout: 15_000,
    })
    .toMatch(/sync \d+ ms/);

  await ctxA.close();
  await ctxB.close();
});
