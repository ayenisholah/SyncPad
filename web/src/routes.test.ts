import { describe, expect, it } from "vitest";
import { parseDocId } from "./routes";

describe("parseDocId", () => {
  it("extracts the slug from an editor path", () => {
    expect(parseDocId("/d/x7k2p9q1")).toBe("x7k2p9q1");
  });

  it("accepts a trailing slash", () => {
    expect(parseDocId("/d/x7k2p9q1/")).toBe("x7k2p9q1");
  });

  it("returns null for non-editor paths", () => {
    expect(parseDocId("/")).toBeNull();
    expect(parseDocId("/docs")).toBeNull();
    expect(parseDocId("/d/")).toBeNull();
  });

  it("rejects characters outside the slug alphabet", () => {
    expect(parseDocId("/d/UPPERCAS")).toBeNull();
    expect(parseDocId("/d/x7k2p9q1/extra")).toBeNull();
    expect(parseDocId("/d/x7k2..q1")).toBeNull();
  });
});
