import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setAuthToken } from "../client";
import {
  deleteInstallerMedia,
  listInstallerMedia,
  recordInstallerMediaVerification,
  setDefaultInstallerMedia,
  uploadInstallerMedia,
} from "./installerMedia";

beforeEach(() => {
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(new Response("{}"))));
});

afterEach(() => vi.restoreAllMocks());

describe("installer media API", () => {
  it("uploads a file with the selected media type and browser multipart boundary", async () => {
    const file = new File(["iso-bytes"], "winpe.iso", { type: "application/octet-stream" });
    fetch.mockResolvedValueOnce(new Response(JSON.stringify({ id: "media-1", media_type: "winpe" }), {
      headers: { "Content-Type": "application/json" },
    }));
    const result = await uploadInstallerMedia({ displayName: "WinPE", mediaType: "winpe", file });

    const [url, request] = fetch.mock.calls[0];
    expect(url).toBe("/api/installer-media/upload");
    expect(request.method).toBe("POST");
    expect(request.headers["Content-Type"]).toBeUndefined();
    expect(request.body.get("display_name")).toBe("WinPE");
    expect(request.body.get("media_type")).toBe("winpe");
    expect(request.body.get("file")).toBe(file);
    expect(result).toMatchObject({ id: "media-1", media_type: "winpe" });
  });

  it("uses catalog routes for default, delete, and boot-verification results", async () => {
    await listInstallerMedia();
    await setDefaultInstallerMedia("media/1");
    await deleteInstallerMedia("media/1");
    const verification = {
      checksum: "abc",
      firmware_mode: "uefi",
      test_client_id: "client-1",
      assigned_image_revision: "pool/image@rev-1",
      observed_luns: [0, 1],
      boot_result: "passed",
    };
    await recordInstallerMediaVerification("media/1", verification);

    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      "/api/installer-media",
      "/api/installer-media/media%2F1/default",
      "/api/installer-media/media%2F1",
      "/api/installer-media/media%2F1/verification",
    ]);
    expect(fetch.mock.calls[1][1].method).toBe("POST");
    expect(fetch.mock.calls[2][1].method).toBe("DELETE");
    expect(fetch.mock.calls[3][1]).toMatchObject({
      method: "POST",
      body: JSON.stringify(verification),
    });
  });
});
