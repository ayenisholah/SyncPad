import { test, expect, type Page } from "@playwright/test";

// The convergence guarantee, proven in real browsers (gate M2): concurrent
// edits from two pages end byte-identical, and a late joiner sees the result.

/** Read the current Monaco model content from a page. */
async function readContent(page: Page): Promise<string> {
  return page.evaluate(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    const model = monaco?.editor?.getModels?.()[0];
    return model ? (model.getValue() as string) : "";
  });
}

/** Wait until Monaco has mounted a model and the socket has seeded it. */
async function waitForEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().waitFor({ state: "visible" });
  await page.waitForFunction(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    return Boolean(monaco?.editor?.getModels?.().length);
  });
  // The connection status flips to "connected" once init arrives.
  await expect(page.locator(".editor-status")).toContainText("connected");
}

/** Click the editor to focus its hidden textarea. */
async function focusEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().click();
}

/** Type at the current caret (the editor must already be focused). */
async function typeInto(page: Page, text: string): Promise<void> {
  await page.keyboard.type(text, { delay: 15 });
}

/** Focus the editor and move the caret to the very start of the document. */
async function caretToStart(page: Page): Promise<void> {
  await focusEditor(page);
  await page.keyboard.press("ControlOrMeta+Home");
}

/** Focus the editor and move the caret to the very end of the document. */
async function caretToEnd(page: Page): Promise<void> {
  await focusEditor(page);
  await page.keyboard.press("ControlOrMeta+End");
}

async function createDoc(page: Page): Promise<string> {
  const response = await page.request.post("/api/docs");
  expect(response.ok()).toBeTruthy();
  const body = (await response.json()) as { docId: string };
  return body.docId;
}

/** Poll until both pages report identical, non-empty content. */
async function expectConverged(a: Page, b: Page): Promise<string> {
  let last = "";
  await expect
    .poll(
      async () => {
        const [ca, cb] = [await readContent(a), await readContent(b)];
        last = ca;
        return ca.length > 0 && ca === cb ? "converged" : `diverged(${ca}|${cb})`;
      },
      { timeout: 15_000 },
    )
    .toBe("converged");
  return last;
}

test("concurrent same-offset typing converges in two browsers", async ({ browser }) => {
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const a = await ctxA.newPage();
  const b = await ctxB.newPage();

  const docId = await createDoc(a);
  await a.goto(`/d/${docId}`);
  await b.goto(`/d/${docId}`);
  await waitForEditor(a);
  await waitForEditor(b);

  // Both carets at offset 0, then interleaved bursts — genuine concurrency
  // through the ack/transform/buffer paths.
  await caretToStart(a);
  await caretToStart(b);
  for (let round = 0; round < 4; round += 1) {
    await Promise.all([typeInto(a, "AAA"), typeInto(b, "bbb")]);
    await a.waitForTimeout(120);
  }

  const converged = await expectConverged(a, b);
  expect(converged.length).toBeGreaterThan(0);
  // Every typed character survived (nothing lost), regardless of interleaving.
  expect(converged.replace(/[^A]/g, "").length).toBe(12);
  expect(converged.replace(/[^b]/g, "").length).toBe(12);

  await ctxA.close();
  await ctxB.close();
});

test("a late joiner sees the converged content", async ({ browser }) => {
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const a = await ctxA.newPage();
  const b = await ctxB.newPage();

  const docId = await createDoc(a);
  await a.goto(`/d/${docId}`);
  await b.goto(`/d/${docId}`);
  await waitForEditor(a);
  await waitForEditor(b);

  await caretToStart(a);
  await typeInto(a, "shared content");
  const converged = await expectConverged(a, b);

  // A third page joining afterward is seeded from the server's snapshot state.
  const ctxC = await browser.newContext();
  const c = await ctxC.newPage();
  await c.goto(`/d/${docId}`);
  await waitForEditor(c);
  await expect.poll(async () => readContent(c), { timeout: 15_000 }).toBe(converged);

  await ctxA.close();
  await ctxB.close();
  await ctxC.close();
});

test("edits relay both directions", async ({ browser }) => {
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const a = await ctxA.newPage();
  const b = await ctxB.newPage();

  const docId = await createDoc(a);
  await a.goto(`/d/${docId}`);
  await b.goto(`/d/${docId}`);
  await waitForEditor(a);
  await waitForEditor(b);

  await caretToStart(a);
  await typeInto(a, "hello");
  await expect.poll(async () => readContent(b), { timeout: 15_000 }).toContain("hello");

  // B appends; A sees it.
  await caretToEnd(b);
  await typeInto(b, " world");
  await expect.poll(async () => readContent(a), { timeout: 15_000 }).toContain("world");

  await expectConverged(a, b);

  await ctxA.close();
  await ctxB.close();
});
