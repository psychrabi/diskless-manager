import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setAuthToken } from "../client";
import {
  deleteInstallerMedia,
  listInstallerMedia,
  recordInstallerMediaVerification,
  setDefaultInstallerMedia,
  uploadInstallerMedia,
} from "./installerMedia";

class FakeXMLHttpRequest {
  static instances = [];

  constructor() {
    this.upload = {};
    this.headers = {};
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    FakeXMLHttpRequest.instances.push(this);
  }

  open(method, url) {
    this.method = method;
    this.url = url;
  }

  setRequestHeader(name, value) {
    this.headers[name] = value;
  }

  getResponseHeader(name) {
    return name.toLowerCase() === "content-type" ? "application/json" : null;
  }

  send(body) {
    this.body = body;
  }

  respond({ status = 201, statusText = "Created", body = {} } = {}) {
    this.status = status;
    this.statusText = statusText;
    this.responseText = JSON.stringify(body);
    this.onload();
  }
}

beforeEach(() => {
  FakeXMLHttpRequest.instances = [];
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(new Response("{}"))));
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("installer media API", () => {
  it("uploads a file with live progress and the selected media type", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    const file = new File(["iso-bytes"], "winpe.iso", { type: "application/octet-stream" });
    const onProgress = vi.fn();
    const result = uploadInstallerMedia({ displayName: "WinPE", mediaType: "winpe", file, onProgress });

    const request = FakeXMLHttpRequest.instances[0];
    expect(request.method).toBe("POST");
    expect(request.url).toBe("/api/installer-media/upload");
    expect(request.headers.Authorization).toBe("Bearer admin-token");
    expect(request.body.get("display_name")).toBe("WinPE");
    expect(request.body.get("media_type")).toBe("winpe");
    expect(request.body.get("file")).toBe(file);

    request.upload.onprogress({ lengthComputable: true, loaded: 512, total: 2048 });
    expect(onProgress).toHaveBeenLastCalledWith(0.25);
    request.upload.onload();
    expect(onProgress).toHaveBeenLastCalledWith(1);

    request.respond({ body: { id: "media-1", media_type: "winpe" } });
    await expect(result).resolves.toMatchObject({ id: "media-1", media_type: "winpe" });
  });

  it("surfaces the API error message for a rejected upload", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    const file = new File(["iso-bytes"], "winpe.iso", { type: "application/octet-stream" });
    const result = uploadInstallerMedia({ displayName: "WinPE", mediaType: "winpe", file });

    FakeXMLHttpRequest.instances[0].respond({
      status: 409,
      statusText: "Conflict",
      body: { code: "media_in_use", message: "Media is selected or in use" },
    });

    await expect(result).rejects.toMatchObject({
      message: "Media is selected or in use",
      status: 409,
    });
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
