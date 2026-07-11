/** Create a new document and return its id. */
export async function createDoc(): Promise<string> {
  const response = await fetch("/api/docs", { method: "POST" });
  if (!response.ok) {
    throw new Error(`document creation failed: ${response.status}`);
  }
  const body = (await response.json()) as { docId: string };
  return body.docId;
}
