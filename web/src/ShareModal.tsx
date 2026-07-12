import { useEffect, useRef, useState } from "react";
import { useMonaco } from "@monaco-editor/react";
import { toBlob, toPng } from "html-to-image";
import { buildShareLinks, filenameForLanguage, shareText } from "./share";

const SHARE_TITLE = "Made with SyncPad";
/** Cap the card height so a whole large document still makes a sane image. */
const MAX_LINES = 48;

export function ShareModal({
  docUrl,
  language,
  languageLabel,
  selection,
  whole,
  onClose,
}: {
  docUrl: string;
  language: string;
  languageLabel: string;
  selection: string;
  whole: string;
  onClose: () => void;
}) {
  const monaco = useMonaco();
  const cardRef = useRef<HTMLDivElement>(null);
  const [html, setHtml] = useState("");
  const [busy, setBusy] = useState(false);
  const [copiedLink, setCopiedLink] = useState(false);
  const [copiedImg, setCopiedImg] = useState(false);

  const full = shareText(selection, whole);
  const lines = full.split("\n");
  const code = lines.length > MAX_LINES ? lines.slice(0, MAX_LINES).join("\n") + "\n…" : full;
  const links = buildShareLinks(docUrl, SHARE_TITLE);

  // Syntax-highlight the snippet by reusing Monaco's colorizer (D-008), which
  // emits spans styled by the active theme already loaded on the page.
  useEffect(() => {
    let alive = true;
    if (!monaco) return;
    monaco.editor
      .colorize(code, language, { tabSize: 2 })
      .then((out) => alive && setHtml(out))
      .catch(() => alive && setHtml(""));
    return () => {
      alive = false;
    };
  }, [monaco, code, language]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const download = async () => {
    const node = cardRef.current;
    if (!node) return;
    setBusy(true);
    try {
      const url = await toPng(node, { pixelRatio: 2, cacheBust: true });
      const a = document.createElement("a");
      a.href = url;
      a.download = filenameForLanguage(language).replace(/\.\w+$/, ".png");
      a.click();
    } finally {
      setBusy(false);
    }
  };

  const copyImage = async () => {
    const node = cardRef.current;
    if (!node) return;
    setBusy(true);
    try {
      const blob = await toBlob(node, { pixelRatio: 2, cacheBust: true });
      if (blob && navigator.clipboard && "write" in navigator.clipboard) {
        await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
        setCopiedImg(true);
        setTimeout(() => setCopiedImg(false), 1500);
      }
    } catch {
      /* image clipboard unsupported here — Download PNG still works */
    } finally {
      setBusy(false);
    }
  };

  const copyLink = () => {
    void navigator.clipboard?.writeText(docUrl).then(() => {
      setCopiedLink(true);
      setTimeout(() => setCopiedLink(false), 1500);
    });
  };

  const openIntent = (url: string) => window.open(url, "_blank", "noopener,noreferrer");

  return (
    <div className="share-overlay" onClick={onClose}>
      <div className="share-modal" onClick={(e) => e.stopPropagation()}>
        <div className="share-modal-head">
          <h2>Share code</h2>
          <button className="share-close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>

        <div className="share-card" ref={cardRef}>
          <div className="share-card-head">
            <span className="share-chip">{languageLabel}</span>
            <span className="share-brand">
              <img src="/favicon.svg" alt="" width={16} height={16} />
              syncpad.sholaayeni.xyz
            </span>
          </div>
          <pre className="share-code">
            {html ? <code dangerouslySetInnerHTML={{ __html: html }} /> : <code>{code}</code>}
          </pre>
        </div>

        <div className="share-actions">
          <button className="share-action primary" onClick={() => void download()} disabled={busy}>
            Download PNG
          </button>
          <button className="share-action" onClick={() => void copyImage()} disabled={busy}>
            {copiedImg ? "Copied ✓" : "Copy image"}
          </button>
          <button className="share-action" onClick={copyLink}>
            {copiedLink ? "Copied ✓" : "Copy link"}
          </button>
          <span className="share-sep" />
          <button className="share-action" onClick={() => openIntent(links.x)}>
            Share on X
          </button>
          <button className="share-action" onClick={() => openIntent(links.linkedin)}>
            LinkedIn
          </button>
          <button className="share-action" onClick={() => openIntent(links.reddit)}>
            Reddit
          </button>
        </div>
      </div>
    </div>
  );
}
