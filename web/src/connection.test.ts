import { describe, expect, it } from "vitest";
import { Connection, type SocketLike } from "./connection";
import { TextOperation, insertAt } from "./ops";

/** A controllable in-memory socket for driving the connection in tests. */
class FakeSocket implements SocketLike {
  sent: string[] = [];
  onopen: ((event: unknown) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.onclose?.(undefined);
  }
  /** Simulate a server → client message. */
  deliver(message: unknown): void {
    this.onmessage?.({ data: JSON.stringify(message) });
  }
}

function connect() {
  const socket = new FakeSocket();
  const applied: TextOperation[] = [];
  let init: unknown = null;
  let resynced = false;
  let language: string | null = null;
  const connection = new Connection(
    "ws://test/ws/doc",
    {
      onInit: (state) => (init = state),
      onApplyOperation: (op) => applied.push(op),
      onResync: () => (resynced = true),
      onLanguage: (lang) => (language = lang),
    },
    () => socket,
  );
  connection.connect();
  socket.onopen?.(undefined);
  return {
    connection,
    socket,
    applied,
    getInit: () => init,
    wasResynced: () => resynced,
    getLanguage: () => language,
  };
}

function seedInit(ctx: ReturnType<typeof connect>, revision = 0): void {
  ctx.socket.deliver({
    type: "init",
    revision,
    content: "",
    language: "plaintext",
    participants: [],
    selfId: "self-1",
  });
}

describe("Connection", () => {
  it("seeds state from init", () => {
    const ctx = connect();
    expect(ctx.getInit()).toBeNull(); // nothing until the server sends init

    ctx.socket.deliver({
      type: "init",
      revision: 4,
      content: "hello",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });
    expect(ctx.getInit()).toMatchObject({ revision: 4, content: "hello", selfId: "self-1" });
    expect(ctx.connection.revision).toBe(4);
  });

  it("sends a local edit as an op based on the current revision", () => {
    const ctx = connect();
    ctx.socket.deliver({
      type: "init",
      revision: 2,
      content: "ab",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });

    ctx.connection.submit(insertAt(2, 0, "x"));
    expect(ctx.socket.sent).toHaveLength(1);
    const sent = JSON.parse(ctx.socket.sent[0]);
    expect(sent.type).toBe("op");
    expect(sent.baseRevision).toBe(2);
    expect(sent.ops).toEqual(["x", 2]); // retain(0) is dropped
  });

  it("applies a transformed remote op and advances the revision", () => {
    const ctx = connect();
    ctx.socket.deliver({
      type: "init",
      revision: 0,
      content: "ab",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });

    ctx.socket.deliver({ type: "op", revision: 1, ops: [2, "y"], authorId: "other", sentAt: 0 });
    expect(ctx.applied).toHaveLength(1);
    expect(ctx.applied[0].apply("ab")).toBe("aby");
    expect(ctx.connection.revision).toBe(1);
  });

  it("promotes on ack", () => {
    const ctx = connect();
    ctx.socket.deliver({
      type: "init",
      revision: 0,
      content: "",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });
    ctx.connection.submit(insertAt(0, 0, "hi"));
    ctx.socket.deliver({ type: "ack", revision: 1 });
    expect(ctx.connection.revision).toBe(1);
  });

  it("drops state and re-initializes on resync", () => {
    const ctx = connect();
    ctx.socket.deliver({
      type: "init",
      revision: 5,
      content: "old",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });
    ctx.socket.deliver({ type: "resync" });
    expect(ctx.wasResynced()).toBe(true);

    // A submit before the next init is safely ignored.
    ctx.connection.submit(insertAt(3, 0, "x"));
    expect(ctx.socket.sent).toHaveLength(0);

    ctx.socket.deliver({
      type: "init",
      revision: 6,
      content: "new",
      language: "plaintext",
      participants: [],
      selfId: "self-1",
    });
    expect(ctx.connection.revision).toBe(6);
  });

  it("sends a setLanguage frame and surfaces language changes", () => {
    const ctx = connect();
    seedInit(ctx);

    ctx.connection.sendLanguage("rust");
    expect(JSON.parse(ctx.socket.sent[0])).toEqual({ type: "setLanguage", language: "rust" });

    ctx.socket.deliver({ type: "language", language: "python" });
    expect(ctx.getLanguage()).toBe("python");
  });

  it("samples op→apply latency from sentAt", () => {
    const ctx = connect();
    seedInit(ctx);
    expect(ctx.connection.latencyP50()).toBeNull();

    const now = Date.now();
    // Three remote ops with known ages → median is the middle age.
    ctx.socket.deliver({ type: "op", revision: 1, ops: ["x"], authorId: "o", sentAt: now - 10 });
    ctx.socket.deliver({ type: "op", revision: 2, ops: [1, "y"], authorId: "o", sentAt: now - 30 });
    ctx.socket.deliver({ type: "op", revision: 3, ops: [2, "z"], authorId: "o", sentAt: now - 20 });

    const p50 = ctx.connection.latencyP50();
    expect(p50).not.toBeNull();
    expect(p50!).toBeGreaterThanOrEqual(15);
    expect(p50!).toBeLessThanOrEqual(30);
  });
});
