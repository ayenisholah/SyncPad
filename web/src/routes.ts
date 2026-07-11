// Document slugs come from the server's 32-character alphabet (digits plus
// lowercase letters without i, l, o, u).
const EDITOR_PATH = /^\/d\/([0-9a-z]+)\/?$/;

/** Extract the document id from an editor path like `/d/x7k2p9q1`. */
export function parseDocId(pathname: string): string | null {
  const match = EDITOR_PATH.exec(pathname);
  return match?.[1] ?? null;
}
