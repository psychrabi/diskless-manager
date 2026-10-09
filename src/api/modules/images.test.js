import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { setAuthToken } from "../client";
import { createSnapshot } from "./images";

beforeEach(() => {
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}")));
});

afterEach(() => vi.restoreAllMocks());

it("encodes a ZFS dataset path when creating a snapshot", async () => {
  await createSnapshot("diskless/image-disk/windows11", "save-1");

  expect(fetch).toHaveBeenCalledWith(
    "/api/images/diskless%2Fimage-disk%2Fwindows11/snapshots",
    expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ snapshot_name: "save-1" }),
    }),
  );
});
