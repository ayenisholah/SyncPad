import { test, expect, type Page } from "@playwright/test";

// Remote cursors track the right position during concurrent edits (gate for
// W2D3-1): a peer's caret decoration follows the text as it shifts, and is
// removed when the peer leaves.

async function waitForEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().waitFor({ state: "visible" });
  await page.waitForFunction(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    return Boolean(monaco?.editor?.getModels?.().length);
  });
  await expect(page.locator(".editor-status")).toContainText("connected");
}

async function readContent(page: Page): Promise<string> {
  return page.evaluate(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    const model = monaco?.editor?.getModels?.()[0];
    return model ? (model.getValue() as string) : "";
  });
}

/** The UTF-16 offset of the remote-cursor caret decoration, or null if none. */
async function remoteCaretOffset(page: Page): Promise<number | null> {
  return page.evaluate(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    const model = monaco?.editor?.getModels?.()[0];
    if (!model) return null;
    const caret = model
      .getAllDecorations()
      .find((d: any) => String(d.options.className ?? "").startsWith("rc-"));
    if (!caret) return null;
    return model.getOffsetAt({
      lineNumber: caret.range.startLineNumber,
      column: caret.range.startColumn,
    });
  });
}

async function focusEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().click();
}

async function createDoc(page: Page): Promise<string> {
  const response = await page.request.post("/api/docs");
  expect(response.ok()).toBeTruthy();
  return ((await response.json()) as { docId: string }).docId;
}

test("a remote cursor follows text inserted before it, then clears on leave", async ({
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

  // A seeds shared content; B waits to see it.
  await focusEditor(a);
  await a.keyboard.type("hello world", { delay: 10 });
  await expect.poll(() => readContent(b), { timeout: 15_000 }).toBe("hello world");

  // B puts its caret at offset 5 (just after "hello").
  await focusEditor(b);
  await b.keyboard.press("ControlOrMeta+Home");
  for (let i = 0; i < 5; i += 1) await b.keyboard.press("ArrowRight");

  // A shows B's caret decoration at offset 5.
  await expect.poll(() => remoteCaretOffset(a), { timeout: 15_000 }).toBe(5);

  // A inserts two characters at the very start; B's caret decoration in A's
  // editor must shift right by two, to offset 7.
  await focusEditor(a);
  await a.keyboard.press("ControlOrMeta+Home");
  await a.keyboard.type("XX", { delay: 10 });
  await expect.poll(() => remoteCaretOffset(a), { timeout: 15_000 }).toBe(7);

  // When B leaves, its decoration is removed from A's editor.
  await ctxB.close();
  await expect.poll(() => remoteCaretOffset(a), { timeout: 15_000 }).toBeNull();

  await ctxA.close();
});
