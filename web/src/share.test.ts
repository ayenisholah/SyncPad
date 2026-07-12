import { describe, expect, it } from "vitest";
import { buildShareLinks, filenameForLanguage, shareText } from "./share";

describe("filenameForLanguage", () => {
  it("maps known languages to their extension", () => {
    expect(filenameForLanguage("rust")).toBe("snippet.rs");
    expect(filenameForLanguage("typescript")).toBe("snippet.ts");
    expect(filenameForLanguage("python")).toBe("snippet.py");
  });

  it("falls back to .txt for unknown languages", () => {
    expect(filenameForLanguage("brainfuck")).toBe("snippet.txt");
    expect(filenameForLanguage("plaintext")).toBe("snippet.txt");
  });
});

describe("shareText", () => {
  it("prefers a non-empty selection", () => {
    expect(shareText("let x = 1;", "whole document")).toBe("let x = 1;");
  });

  it("falls back to the whole document when the selection is blank", () => {
    expect(shareText("   \n ", "whole document")).toBe("whole document");
    expect(shareText("", "whole document")).toBe("whole document");
  });

  it("trims trailing whitespace", () => {
    expect(shareText("code\n\n  ", "")).toBe("code");
  });
});

describe("buildShareLinks", () => {
  it("encodes the url and title into each intent", () => {
    const links = buildShareLinks("https://syncpad.sholaayeni.xyz/d/ab12", "Made with SyncPad");
    const u = encodeURIComponent("https://syncpad.sholaayeni.xyz/d/ab12");
    expect(links.x).toContain(`url=${u}`);
    expect(links.x).toContain("text=Made%20with%20SyncPad");
    expect(links.linkedin).toBe(`https://www.linkedin.com/sharing/share-offsite/?url=${u}`);
    expect(links.reddit).toContain(`url=${u}`);
    expect(links.reddit).toContain("title=Made%20with%20SyncPad");
  });
});
