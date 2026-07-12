import { useEffect } from "react";
import { Editor } from "./Editor";
import { Landing } from "./Landing";
import { parseDocId } from "./routes";

export function App() {
  const docId = parseDocId(window.location.pathname);

  // Documents live behind unguessable links and expire — never let a doc route
  // be indexed (D-009), and reflect the doc id in the tab title.
  useEffect(() => {
    if (!docId) return;
    const robots = document.querySelector('meta[name="robots"]');
    const previousRobots = robots?.getAttribute("content") ?? null;
    robots?.setAttribute("content", "noindex, nofollow");
    const previousTitle = document.title;
    document.title = `SyncPad · d/${docId}`;
    return () => {
      if (robots && previousRobots !== null) robots.setAttribute("content", previousRobots);
      document.title = previousTitle;
    };
  }, [docId]);

  return docId ? <Editor docId={docId} /> : <Landing />;
}
