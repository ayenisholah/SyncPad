export function Editor({ docId }: { docId: string }) {
  return (
    <main className="shell">
      <h1 className="wordmark">SyncPad</h1>
      <p className="tagline">
        Document <code>d/{docId}</code>
      </p>
      <p className="note">The collaborative editor lands here next.</p>
    </main>
  );
}
