import { Editor } from "./Editor";
import { Landing } from "./Landing";
import { parseDocId } from "./routes";

export function App() {
  const docId = parseDocId(window.location.pathname);
  return docId ? <Editor docId={docId} /> : <Landing />;
}
