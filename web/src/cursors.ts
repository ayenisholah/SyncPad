// Remote cursor and selection decorations in Monaco (spec §6.3, FR5).
//
// Each participant gets a colored caret bar with a name label and, when they
// have a selection, a translucent range highlight. Decoration ids are cached
// per user so an update replaces that user's decorations without disturbing
// others (the §6.3 "decoration id caching per user" trap). Per-user color is
// injected as CSS keyed by the user id.

import type * as monaco from "monaco-editor";
import { codePointToUtf16 } from "./ops";
import type { Selection } from "./connection";

type Monaco = typeof monaco;

/** CSS-class prefix for a user's decorations. */
function classFor(userId: string): string {
  return `rc-${userId}`;
}

export class RemoteCursors {
  private readonly decorations = new Map<string, string[]>();
  private readonly styled = new Set<string>();
  private readonly styleEl: HTMLStyleElement;

  constructor(
    private readonly editor: monaco.editor.IStandaloneCodeEditor,
    private readonly monaco: Monaco,
  ) {
    this.styleEl = document.createElement("style");
    this.styleEl.dataset.syncpad = "remote-cursors";
    document.head.appendChild(this.styleEl);
  }

  /** Add or update a participant's caret and selection. */
  set(
    userId: string,
    name: string,
    color: string,
    position: number,
    selection?: Selection,
  ): void {
    const model = this.editor.getModel();
    if (!model) return;
    this.ensureStyle(userId, color);

    const cls = classFor(userId);
    const text = model.getValue();
    const toPosition = (offset: number) =>
      model.getPositionAt(codePointToUtf16(text, offset));

    const caret = toPosition(position);
    const decorations: monaco.editor.IModelDeltaDecoration[] = [
      {
        range: new this.monaco.Range(
          caret.lineNumber,
          caret.column,
          caret.lineNumber,
          caret.column,
        ),
        options: {
          className: `${cls}-caret`,
          stickiness:
            this.monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
          before: { content: "​", inlineClassName: `${cls}-bar` },
          after: { content: ` ${name} `, inlineClassName: `${cls}-label` },
        },
      },
    ];

    if (selection && selection.anchor !== selection.head) {
      const start = toPosition(Math.min(selection.anchor, selection.head));
      const end = toPosition(Math.max(selection.anchor, selection.head));
      decorations.push({
        range: new this.monaco.Range(
          start.lineNumber,
          start.column,
          end.lineNumber,
          end.column,
        ),
        options: { className: `${cls}-selection` },
      });
    }

    const previous = this.decorations.get(userId) ?? [];
    this.decorations.set(userId, this.editor.deltaDecorations(previous, decorations));
  }

  /** Remove a participant's decorations (e.g. on leave). */
  remove(userId: string): void {
    const previous = this.decorations.get(userId);
    if (previous) this.editor.deltaDecorations(previous, []);
    this.decorations.delete(userId);
  }

  /** Remove every remote decoration and the injected styles. */
  clear(): void {
    for (const userId of [...this.decorations.keys()]) this.remove(userId);
    this.styled.clear();
    this.styleEl.textContent = "";
  }

  /** Dispose the injected style element. */
  dispose(): void {
    this.clear();
    this.styleEl.remove();
  }

  private ensureStyle(userId: string, color: string): void {
    if (this.styled.has(userId)) return;
    this.styled.add(userId);
    const cls = classFor(userId);
    // `color` is a 7-char hex from the server palette; append alpha for the
    // selection highlight.
    this.styleEl.textContent += `
.${cls}-bar { border-left: 2px solid ${color}; margin-left: -1px; }
.${cls}-label {
  background: ${color}; color: #1e1e1e; border-radius: 3px;
  font-size: 0.72em; font-weight: 600; padding: 0 1px; vertical-align: baseline;
}
.${cls}-selection { background: ${color}33; }
`;
  }
}
