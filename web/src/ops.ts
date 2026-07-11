// Client-side operation algebra.
//
// A minimal TypeScript port of the ot.js `TextOperation` (apply / compose /
// transform), matching the semantics of the server's `operational-transform`
// crate so both sides agree on every transform. The wire format is the crate's
// flat-array serialization: a positive number is a retain, a negative number a
// delete, and a string an insert — e.g. `[5, "x", -3]`.
//
// Counting is by Unicode scalar value (code point), not UTF-16 code unit, to
// match the crate's `chars()`-based lengths; astral characters (e.g. an emoji)
// count as one. Keep this module free of editor/DOM concerns: it is the
// correctness core and is unit-tested in isolation. The intent is that a future
// wasm build of the crate can replace it behind the same interface.

/** A single component: retain (n > 0), delete (n < 0), or insert (string). */
export type Component = number | string;

/** The wire form of an operation (the crate's flat-array serialization). */
export type SerializedOperation = Component[];

/** Code-point length of a string (not UTF-16 units). */
function cpLength(str: string): number {
  let count = 0;
  for (const _ of str) count += 1;
  return count;
}

/** Slice a string by code-point indices. */
function cpSlice(str: string, start: number, end?: number): string {
  return Array.from(str).slice(start, end).join("");
}

const isRetain = (op: Component | undefined): op is number =>
  typeof op === "number" && op > 0;
const isDelete = (op: Component | undefined): op is number =>
  typeof op === "number" && op < 0;
const isInsert = (op: Component | undefined): op is string =>
  typeof op === "string";

export class TextOperation {
  /** Components in order; also the wire representation. */
  ops: Component[] = [];
  /** Length of the string this operation can be applied to. */
  baseLength = 0;
  /** Length of the string produced by applying this operation. */
  targetLength = 0;

  /** Keep `count` characters from the input unchanged. */
  retain(count: number): this {
    if (count === 0) return this;
    this.baseLength += count;
    this.targetLength += count;
    const last = this.ops[this.ops.length - 1];
    if (isRetain(last)) {
      this.ops[this.ops.length - 1] = last + count;
    } else {
      this.ops.push(count);
    }
    return this;
  }

  /** Insert `str` at the current position. */
  insert(str: string): this {
    if (str === "") return this;
    this.targetLength += cpLength(str);
    const ops = this.ops;
    const last = ops[ops.length - 1];
    if (isInsert(last)) {
      ops[ops.length - 1] = last + str;
    } else if (isDelete(last)) {
      // Keep inserts before deletes so a delete never precedes an insert.
      const secondLast = ops[ops.length - 2];
      if (isInsert(secondLast)) {
        ops[ops.length - 2] = secondLast + str;
      } else {
        ops[ops.length - 1] = str;
        ops.push(last);
      }
    } else {
      ops.push(str);
    }
    return this;
  }

  /** Delete `count` characters from the input. */
  delete(count: number): this {
    if (count === 0) return this;
    let n = count > 0 ? -count : count;
    this.baseLength += -n;
    const last = this.ops[this.ops.length - 1];
    if (isDelete(last)) {
      this.ops[this.ops.length - 1] = last + n;
    } else {
      this.ops.push(n);
    }
    return this;
  }

  /** Whether this operation changes nothing (a bare retain of everything). */
  isNoop(): boolean {
    return this.ops.length === 0 || (this.ops.length === 1 && isRetain(this.ops[0]));
  }

  /** Apply this operation to `input`, producing the transformed string. */
  apply(input: string): string {
    const chars = Array.from(input);
    if (chars.length !== this.baseLength) {
      throw new Error("apply: operation base length does not match input length");
    }
    const out: string[] = [];
    let index = 0;
    for (const op of this.ops) {
      if (isRetain(op)) {
        if (index + op > chars.length) {
          throw new Error("apply: operation retains past the end of the input");
        }
        for (let i = 0; i < op; i += 1) out.push(chars[index + i]);
        index += op;
      } else if (isInsert(op)) {
        out.push(op);
      } else {
        index += -op;
      }
    }
    if (index !== chars.length) {
      throw new Error("apply: operation did not consume the whole input");
    }
    return out.join("");
  }

  /**
   * Compose with `other` so that `apply(apply(s, this), other)` equals
   * `apply(s, this.compose(other))`. `this` is applied first.
   */
  compose(other: TextOperation): TextOperation {
    if (this.targetLength !== other.baseLength) {
      throw new Error("compose: first operation's target length must match the second's base");
    }
    const result = new TextOperation();
    const ops1 = this.ops;
    const ops2 = other.ops;
    let i1 = 0;
    let i2 = 0;
    let op1: Component | undefined = ops1[i1++];
    let op2: Component | undefined = ops2[i2++];
    for (;;) {
      if (op1 === undefined && op2 === undefined) break;

      if (isDelete(op1)) {
        result.delete(op1);
        op1 = ops1[i1++];
        continue;
      }
      if (isInsert(op2)) {
        result.insert(op2);
        op2 = ops2[i2++];
        continue;
      }
      if (op1 === undefined) throw new Error("compose: first operation is too short");
      if (op2 === undefined) throw new Error("compose: first operation is too long");

      if (isRetain(op1) && isRetain(op2)) {
        if (op1 > op2) {
          result.retain(op2);
          op1 = op1 - op2;
          op2 = ops2[i2++];
        } else if (op1 === op2) {
          result.retain(op1);
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          result.retain(op1);
          op2 = op2 - op1;
          op1 = ops1[i1++];
        }
      } else if (isInsert(op1) && isDelete(op2)) {
        const len1 = cpLength(op1);
        if (len1 > -op2) {
          op1 = cpSlice(op1, -op2);
          op2 = ops2[i2++];
        } else if (len1 === -op2) {
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          op2 = op2 + len1;
          op1 = ops1[i1++];
        }
      } else if (isInsert(op1) && isRetain(op2)) {
        const len1 = cpLength(op1);
        if (len1 > op2) {
          result.insert(cpSlice(op1, 0, op2));
          op1 = cpSlice(op1, op2);
          op2 = ops2[i2++];
        } else if (len1 === op2) {
          result.insert(op1);
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          result.insert(op1);
          op2 = op2 - len1;
          op1 = ops1[i1++];
        }
      } else if (isRetain(op1) && isDelete(op2)) {
        if (op1 > -op2) {
          result.delete(op2);
          op1 = op1 + op2;
          op2 = ops2[i2++];
        } else if (op1 === -op2) {
          result.delete(op2);
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          result.delete(-op1);
          op2 = op2 + op1;
          op1 = ops1[i1++];
        }
      } else {
        throw new Error("compose: incompatible operations");
      }
    }
    return result;
  }

  /** Serialize to the crate's flat-array wire form. */
  toJSON(): SerializedOperation {
    return this.ops.slice();
  }

  /** Build an operation from the crate's flat-array wire form. */
  static fromJSON(value: SerializedOperation): TextOperation {
    const op = new TextOperation();
    for (const component of value) {
      if (typeof component === "string") {
        op.insert(component);
      } else if (component > 0) {
        op.retain(component);
      } else if (component < 0) {
        op.delete(component);
      }
      // Zero components are no-ops and skipped.
    }
    return op;
  }

  /**
   * Transform concurrent operations `a` and `b` (both based on the same
   * document) into `[aPrime, bPrime]` such that
   * `apply(apply(s, a), bPrime) === apply(apply(s, b), aPrime)` (TP1).
   */
  static transform(a: TextOperation, b: TextOperation): [TextOperation, TextOperation] {
    if (a.baseLength !== b.baseLength) {
      throw new Error("transform: both operations must have the same base length");
    }
    const aPrime = new TextOperation();
    const bPrime = new TextOperation();
    const ops1 = a.ops;
    const ops2 = b.ops;
    let i1 = 0;
    let i2 = 0;
    let op1: Component | undefined = ops1[i1++];
    let op2: Component | undefined = ops2[i2++];
    for (;;) {
      if (op1 === undefined && op2 === undefined) break;

      if (isInsert(op1)) {
        aPrime.insert(op1);
        bPrime.retain(cpLength(op1));
        op1 = ops1[i1++];
        continue;
      }
      if (isInsert(op2)) {
        aPrime.retain(cpLength(op2));
        bPrime.insert(op2);
        op2 = ops2[i2++];
        continue;
      }
      if (op1 === undefined) throw new Error("transform: first operation is too short");
      if (op2 === undefined) throw new Error("transform: first operation is too long");

      let minl: number;
      if (isRetain(op1) && isRetain(op2)) {
        if (op1 > op2) {
          minl = op2;
          op1 = op1 - op2;
          op2 = ops2[i2++];
        } else if (op1 === op2) {
          minl = op2;
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          minl = op1;
          op2 = op2 - op1;
          op1 = ops1[i1++];
        }
        aPrime.retain(minl);
        bPrime.retain(minl);
      } else if (isDelete(op1) && isDelete(op2)) {
        if (-op1 > -op2) {
          op1 = op1 - op2;
          op2 = ops2[i2++];
        } else if (op1 === op2) {
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          op2 = op2 - op1;
          op1 = ops1[i1++];
        }
      } else if (isDelete(op1) && isRetain(op2)) {
        if (-op1 > op2) {
          minl = op2;
          op1 = op1 + op2;
          op2 = ops2[i2++];
        } else if (-op1 === op2) {
          minl = op2;
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          minl = -op1;
          op2 = op2 + op1;
          op1 = ops1[i1++];
        }
        aPrime.delete(minl);
      } else if (isRetain(op1) && isDelete(op2)) {
        if (op1 > -op2) {
          minl = -op2;
          op1 = op1 + op2;
          op2 = ops2[i2++];
        } else if (op1 === -op2) {
          minl = op1;
          op1 = ops1[i1++];
          op2 = ops2[i2++];
        } else {
          minl = op1;
          op2 = op2 + op1;
          op1 = ops1[i1++];
        }
        bPrime.delete(minl);
      } else {
        throw new Error("transform: incompatible operations");
      }
    }
    return [aPrime, bPrime];
  }
}

/** Build an operation that inserts `text` at code-point `offset`. */
export function insertAt(docLength: number, offset: number, text: string): TextOperation {
  return new TextOperation()
    .retain(offset)
    .insert(text)
    .retain(docLength - offset);
}

/** Build an operation that deletes `count` characters at code-point `offset`. */
export function deleteAt(docLength: number, offset: number, count: number): TextOperation {
  return new TextOperation()
    .retain(offset)
    .delete(count)
    .retain(docLength - offset - count);
}

/**
 * Compose a sequence of operations left to right into one. Each operation's
 * target length must match the next one's base length (they are applied in
 * order to the same evolving document).
 */
export function composeAll(operations: TextOperation[]): TextOperation {
  return operations.reduce((acc, op) => acc.compose(op));
}

/** UTF-16 code-unit offset of a code-point index within `text`. */
export function codePointToUtf16(text: string, codePointIndex: number): number {
  let cp = 0;
  let utf16 = 0;
  for (const ch of text) {
    if (cp >= codePointIndex) break;
    utf16 += ch.length; // 1 for BMP, 2 for astral (surrogate pair)
    cp += 1;
  }
  return utf16;
}

/** Code-point index of a UTF-16 code-unit offset within `text`. */
export function utf16ToCodePoint(text: string, utf16Offset: number): number {
  let cp = 0;
  let utf16 = 0;
  for (const ch of text) {
    if (utf16 >= utf16Offset) break;
    utf16 += ch.length;
    cp += 1;
  }
  return cp;
}

/**
 * Derive an operation from a single-range diff of `oldText` → `newText`
 * (common prefix/suffix, in code points). This turns an editor content change
 * into an operation without depending on the editor's own change deltas; a
 * multi-cursor edit collapses to one coarser-but-valid replace.
 */
export function diffToOperation(oldText: string, newText: string): TextOperation {
  const oldChars = Array.from(oldText);
  const newChars = Array.from(newText);

  let prefix = 0;
  const maxPrefix = Math.min(oldChars.length, newChars.length);
  while (prefix < maxPrefix && oldChars[prefix] === newChars[prefix]) prefix += 1;

  let suffix = 0;
  const maxSuffix = Math.min(oldChars.length - prefix, newChars.length - prefix);
  while (
    suffix < maxSuffix &&
    oldChars[oldChars.length - 1 - suffix] === newChars[newChars.length - 1 - suffix]
  ) {
    suffix += 1;
  }

  const deleteCount = oldChars.length - prefix - suffix;
  const insertText = newChars.slice(prefix, newChars.length - suffix).join("");

  const op = new TextOperation().retain(prefix);
  if (deleteCount > 0) op.delete(deleteCount);
  if (insertText) op.insert(insertText);
  return op.retain(suffix);
}

/** An editor edit in UTF-16 units, ready to map onto Monaco ranges. */
export interface EditSpan {
  /** UTF-16 offset of the edit's start in the pre-edit document. */
  offset: number;
  /** UTF-16 length of the range replaced (0 for a pure insert). */
  length: number;
  /** Replacement text. */
  text: string;
}

/**
 * Convert an operation into edit spans against `doc`, with offsets in UTF-16
 * units (Monaco's coordinate system). All offsets refer to the pre-edit
 * document, so the spans can be applied simultaneously.
 */
export function operationToEdits(operation: TextOperation, doc: string): EditSpan[] {
  const edits: EditSpan[] = [];
  let cp = 0;
  for (const component of operation.ops) {
    if (typeof component === "number") {
      if (component > 0) {
        cp += component; // retain
      } else {
        const start = codePointToUtf16(doc, cp);
        const end = codePointToUtf16(doc, cp - component);
        edits.push({ offset: start, length: end - start, text: "" });
        cp += -component;
      }
    } else {
      const at = codePointToUtf16(doc, cp);
      edits.push({ offset: at, length: 0, text: component });
    }
  }
  return edits;
}
