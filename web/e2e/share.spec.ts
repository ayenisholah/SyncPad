import { test, expect, type Page } from "@playwright/test";

// Social sharing (D-008): the Share panel opens, renders the current snippet as
// a branded card, and offers download / copy / share actions.

async function waitForEditor(page: Page): Promise<void> {
  await page.locator(".monaco-editor").first().waitFor({ state: "visible" });
  await page.waitForFunction(() => {
    const monaco = (window as unknown as { monaco?: any }).monaco;
    return Boolean(monaco?.editor?.getModels?.().length);
  });
  await expect(page.locator(".editor-status")).toContainText("connected");
}

test("Share opens a branded card of the code with share actions", async ({ page }) => {
  const response = await page.request.post("/api/docs");
  expect(response.ok()).toBeTruthy();
  const { docId } = (await response.json()) as { docId: string };

  await page.goto(`/d/${docId}`);
  await waitForEditor(page);

  await page.locator(".lang-select").selectOption("javascript");
  await page.locator(".monaco-editor").first().click();
  await page.keyboard.type("const answer = 42;", { delay: 10 });

  await page.locator(".share-btn").click();

  const modal = page.locator(".share-modal");
  await expect(modal).toBeVisible();
  await expect(modal.locator(".share-chip")).toHaveText("JavaScript");
  await expect(modal.locator(".share-brand")).toContainText("syncpad.sholaayeni.xyz");
  await expect(modal.locator(".share-code")).toContainText("answer");
  await expect(modal.getByRole("button", { name: "Download PNG" })).toBeVisible();
  await expect(modal.getByRole("button", { name: "Share on X" })).toBeVisible();

  // Escape dismisses it.
  await page.keyboard.press("Escape");
  await expect(modal).toBeHidden();
});
