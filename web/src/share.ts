// Pure helpers for sharing a code sample (D-008). No DOM here — the rendering
// and clipboard/image work lives in ShareModal; this module is unit-tested.

/** File extension per Monaco language id (mirrors the editor's allowlist). */
const EXTENSIONS: Record<string, string> = {
  plaintext: "txt",
  javascript: "js",
  typescript: "ts",
  python: "py",
  rust: "rs",
  go: "go",
  java: "java",
  c: "c",
  cpp: "cpp",
  csharp: "cs",
  json: "json",
  yaml: "yml",
  markdown: "md",
  html: "html",
  css: "css",
  sql: "sql",
  shell: "sh",
  ruby: "rb",
  php: "php",
};

/** A download filename for a snippet in the given language, e.g. `snippet.rs`. */
export function filenameForLanguage(language: string): string {
  return `snippet.${EXTENSIONS[language] ?? "txt"}`;
}

/**
 * The text to share: the selection when the user has one, otherwise the whole
 * document. Trailing whitespace is trimmed so the image has no dead space.
 */
export function shareText(selection: string, whole: string): string {
  return (selection.trim().length > 0 ? selection : whole).replace(/\s+$/, "");
}

export interface ShareLinks {
  x: string;
  linkedin: string;
  reddit: string;
}

/** Social share-intent URLs that link back to the document. */
export function buildShareLinks(url: string, title: string): ShareLinks {
  const u = encodeURIComponent(url);
  const t = encodeURIComponent(title);
  return {
    x: `https://twitter.com/intent/tweet?text=${t}&url=${u}`,
    linkedin: `https://www.linkedin.com/sharing/share-offsite/?url=${u}`,
    reddit: `https://www.reddit.com/submit?url=${u}&title=${t}`,
  };
}
