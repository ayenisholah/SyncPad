import { describe, expect, it } from "vitest";
import { OtClient } from "./otClient";
import { TextOperation, insertAt } from "./ops";

/** An OtClient that records what it sends and applies, for assertions. */
function makeClient(revision = 0) {
  const sent: { revision: number; op: TextOperation }[] = [];
  const applied: TextOperation[] = [];
  const client = new OtClient(revision, {
    sendOperation: (r, op) => sent.push({ revision: r, op }),
    applyOperation: (op) => applied.push(op),
  });
  return { client, sent, applied };
}

describe("OtClient transitions", () => {
  it("starts synchronized", () => {
    const { client } = makeClient();
    expect(client.stateName).toBe("synchronized");
  });

  it("sends on a local edit and awaits confirmation", () => {
    const { client, sent } = makeClient(3);
    client.applyClient(insertAt(0, 0, "hi"));
    expect(client.stateName).toBe("awaitingConfirm");
    expect(sent).toHaveLength(1);
    expect(sent[0].revision).toBe(3);
  });

  it("buffers further local edits instead of sending", () => {
    const { client, sent } = makeClient();
    client.applyClient(insertAt(0, 0, "a"));
    client.applyClient(insertAt(1, 1, "b"));
    expect(client.stateName).toBe("awaitingWithBuffer");
    expect(sent).toHaveLength(1); // only the first was sent
  });

  it("returns to synchronized when the outstanding op is acked", () => {
    const { client } = makeClient();
    client.applyClient(insertAt(0, 0, "a"));
    client.serverAck();
    expect(client.stateName).toBe("synchronized");
    expect(client.revision).toBe(1);
  });

  it("sends the buffer on ack when one is pending", () => {
    const { client, sent } = makeClient();
    client.applyClient(insertAt(0, 0, "a"));
    client.applyClient(insertAt(1, 1, "b"));
    client.serverAck();
    expect(client.stateName).toBe("awaitingConfirm");
    expect(client.revision).toBe(1);
    expect(sent).toHaveLength(2);
    expect(sent[1].revision).toBe(1); // buffer sent based on the new revision
  });

  it("applies a remote op directly when synchronized", () => {
    const { client, applied } = makeClient();
    client.applyServer(insertAt(0, 0, "x"));
    expect(client.stateName).toBe("synchronized");
    expect(client.revision).toBe(1);
    expect(applied).toHaveLength(1);
  });

  it("transforms a remote op against the outstanding op", () => {
    const { client, applied } = makeClient();
    // Outstanding: insert "x" at 0 over "ab".
    client.applyClient(new TextOperation().insert("x").retain(2));
    // Remote: insert "y" at 2 over "ab".
    client.applyServer(new TextOperation().retain(2).insert("y"));
    // The applied op, over the client's local view "xab", yields "xaby".
    expect(applied).toHaveLength(1);
    expect(applied[0].apply("xab")).toBe("xaby");
    expect(client.stateName).toBe("awaitingConfirm");
  });
});

describe("OtClient convergence", () => {
  it("two clients converge on the known concurrent case", () => {
    // Both start synchronized at revision 0 over the shared document "ab".
    const base = "ab";
    const a = makeClient();
    const b = makeClient();

    const aOp = new TextOperation().insert("x").retain(2);
    const bOp = new TextOperation().retain(2).insert("y");

    // Each makes its local edit (already reflected in its own editor).
    a.client.applyClient(aOp);
    b.client.applyClient(bOp);
    let aView = aOp.apply(base); // "xab"
    let bView = bOp.apply(base); // "aby"

    // Server order: A committed first at rev 1 (empty log, so peers receive
    // aOp unchanged), then B at rev 2 (transformed against aOp before it is
    // broadcast to A) — exactly what the doc task does.
    const bOpBroadcastToA = TextOperation.transform(bOp, aOp)[0];

    // Client A: its own op is acked, then B's transformed op arrives.
    a.client.serverAck();
    a.client.applyServer(bOpBroadcastToA);
    for (const op of a.applied) aView = op.apply(aView);

    // Client B: A's op arrives while B still awaits its ack, then B is acked.
    b.client.applyServer(aOp);
    for (const op of b.applied) bView = op.apply(bView);
    b.client.serverAck();

    expect(aView).toBe("xaby");
    expect(bView).toBe("xaby");
    expect(a.client.revision).toBe(2);
    expect(b.client.revision).toBe(2);
  });
});
