import { afterEach, describe, expect, it, vi } from "vitest";
import { createDoc } from "./api";

describe("createDoc", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("POSTs to /api/docs and returns the docId", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ docId: "x7k2p9q1" }), { status: 200 }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await expect(createDoc()).resolves.toBe("x7k2p9q1");
    expect(fetchMock).toHaveBeenCalledWith("/api/docs", { method: "POST" });
  });

  it("throws on a non-2xx response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("", { status: 500 })),
    );

    await expect(createDoc()).rejects.toThrow("500");
  });
});
