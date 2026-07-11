import { describe, expect, it } from "vitest";
import {
  TextOperation,
  insertAt,
  deleteAt,
  composeAll,
  diffToOperation,
  operationToEdits,
  codePointToUtf16,
} from "./ops";

/** A small seeded PRNG (mulberry32) so the property tests are reproducible. */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Includes an astral character so code-point (not UTF-16) counting is tested.
const ALPHABET = Array.from("ab \n中🦀");

/** A random operation valid against `doc`, mirroring the Rust fuzz generator. */
function randomOperation(doc: string, rand: () => number): TextOperation {
  const chars = Array.from(doc);
  const op = new TextOperation();
  let i = 0;
  while (i < chars.length) {
    const roll = rand();
    const remaining = chars.length - i;
    if (roll < 0.4) {
      const n = 1 + Math.floor(rand() * remaining);
      op.retain(n);
      i += n;
    } else if (roll < 0.7) {
      const n = 1 + Math.floor(rand() * remaining);
      op.delete(n);
      i += n;
    } else {
      const len = 1 + Math.floor(rand() * 3);
      let text = "";
      for (let k = 0; k < len; k += 1) text += ALPHABET[Math.floor(rand() * ALPHABET.length)];
      op.insert(text);
    }
  }
  // Occasionally insert at the very end.
  if (rand() < 0.3) op.insert(ALPHABET[Math.floor(rand() * ALPHABET.length)]);
  return op;
}

describe("TextOperation.apply", () => {
  it("retains, inserts, and deletes at code-point offsets", () => {
    const op = new TextOperation().retain(2).insert("X").delete(1).retain(1);
    expect(op.apply("abcd")).toBe("abXd");
  });

  it("counts astral characters as one code point", () => {
    // Insert after the crab emoji (one code point), before "z".
    const op = insertAt(2, 1, "!");
    expect(op.apply("🦀z")).toBe("🦀!z");
    expect(deleteAt(2, 0, 1).apply("🦀z")).toBe("z");
  });

  it("throws when the base length does not match", () => {
    expect(() => new TextOperation().retain(5).apply("abc")).toThrow();
  });
});

describe("TextOperation.compose", () => {
  it("equals applying the two operations in sequence", () => {
    const base = "the quick brown fox";
    const a = insertAt(base.length, 4, "very ");
    const afterA = a.apply(base);
    const b = deleteAt(afterA.length, 0, 4);
    expect(a.compose(b).apply(base)).toBe(b.apply(afterA));
  });

  it("composeAll folds a sequence of edits", () => {
    const base = "abc";
    const ops = [insertAt(3, 0, "X"), insertAt(4, 4, "Y"), deleteAt(5, 0, 1)];
    let expected = base;
    for (const op of ops) expected = op.apply(expected);
    expect(composeAll(ops).apply(base)).toBe(expected);
  });
});

describe("TextOperation.transform", () => {
  it("converges on the known concurrent case (ab -> xaby)", () => {
    // From "ab": A inserts "x" at 0, B concurrently inserts "y" at 2.
    const a = new TextOperation().insert("x").retain(2);
    const b = new TextOperation().retain(2).insert("y");
    const [aPrime, bPrime] = TextOperation.transform(a, b);
    // Server order: apply A, then B' — the same content a late joiner sees.
    expect(bPrime.apply(a.apply("ab"))).toBe("xaby");
    // TP1: the other order agrees.
    expect(aPrime.apply(b.apply("ab"))).toBe("xaby");
  });

  it("satisfies TP1 over randomized concurrent operations", () => {
    for (let seed = 1; seed <= 200; seed += 1) {
      const rand = mulberry32(seed);
      let doc = Array.from({ length: 6 }, () => ALPHABET[Math.floor(rand() * ALPHABET.length)]).join("");
      for (let round = 0; round < 20; round += 1) {
        const a = randomOperation(doc, rand);
        const b = randomOperation(doc, rand);
        const [aPrime, bPrime] = TextOperation.transform(a, b);
        const left = bPrime.apply(a.apply(doc));
        const right = aPrime.apply(b.apply(doc));
        expect(left, `seed ${seed} round ${round}`).toBe(right);
        doc = left;
      }
    }
  });
});

describe("offset mapping (editor glue)", () => {
  it("diffToOperation captures a typed insertion", () => {
    const op = diffToOperation("hello world", "hello brave world");
    expect(op.apply("hello world")).toBe("hello brave world");
  });

  it("diffToOperation captures a deletion", () => {
    const op = diffToOperation("hello world", "hello");
    expect(op.apply("hello world")).toBe("hello");
  });

  it("diffToOperation on no change is a noop", () => {
    expect(diffToOperation("same", "same").isNoop()).toBe(true);
  });

  it("codePointToUtf16 accounts for surrogate pairs", () => {
    // "🦀" is two UTF-16 units; code-point index 1 is UTF-16 offset 2.
    expect(codePointToUtf16("🦀z", 1)).toBe(2);
    expect(codePointToUtf16("🦀z", 2)).toBe(3);
  });

  it("operationToEdits yields UTF-16 spans over an astral document", () => {
    // Insert "!" after the emoji (code-point offset 1 → UTF-16 offset 2).
    const op = insertAt(2, 1, "!");
    expect(operationToEdits(op, "🦀z")).toEqual([{ offset: 2, length: 0, text: "!" }]);
    // Delete the emoji (code points 0..1 → UTF-16 0..2).
    const del = deleteAt(2, 0, 1);
    expect(operationToEdits(del, "🦀z")).toEqual([{ offset: 0, length: 2, text: "" }]);
  });
});

describe("TextOperation JSON (crate wire format)", () => {
  it("serializes to the flat array form", () => {
    const op = new TextOperation().retain(10).insert("x").retain(4);
    expect(op.toJSON()).toEqual([10, "x", 4]);
    expect(new TextOperation().retain(3).delete(1).toJSON()).toEqual([3, -1]);
  });

  it("round-trips through fromJSON", () => {
    const op = new TextOperation().retain(2).insert("hi").delete(3).retain(1);
    const back = TextOperation.fromJSON(op.toJSON());
    expect(back.toJSON()).toEqual(op.toJSON());
    expect(back.baseLength).toBe(op.baseLength);
    expect(back.targetLength).toBe(op.targetLength);
  });
});
