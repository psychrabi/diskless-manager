import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { setAuthToken } from "../client";
import {
  listBootTests,
  listDriverRevisions,
  requestInventoryBoot,
  startBootTest,
} from "./networkDrivers";
import { getDriverReadiness } from "./images";

beforeEach(() => {
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}")));
});

afterEach(() => vi.restoreAllMocks());

it("lists driver revisions for a source image", async () => {
  await listDriverRevisions("tank/image/windows");

  expect(fetch).toHaveBeenCalledWith(
    "/api/pxe/windows/revisions?source_image_id=tank%2Fimage%2Fwindows",
    expect.anything(),
  );
});

it("lists boot tests for a candidate revision", async () => {
  await listBootTests("rev-1");

  expect(fetch).toHaveBeenCalledWith(
    "/api/pxe/windows/boot-tests?revision_id=rev-1",
    expect.anything(),
  );
});

it("starts a boot test on the designated client", async () => {
  await startBootTest({
    revisionId: "rev-1",
    testClientId: "client-1",
    expectedNicId: "PCI\\VEN_8086&DEV_15F3",
  });

  expect(fetch).toHaveBeenCalledWith(
    "/api/pxe/windows/boot-tests",
    expect.objectContaining({
      method: "POST",
      body: JSON.stringify({
        revision_id: "rev-1",
        test_client_id: "client-1",
        expected_nic_id: "PCI\\VEN_8086&DEV_15F3",
      }),
    }),
  );
});

it("requests a WinPE inventory boot for a client", async () => {
  await requestInventoryBoot("client-1");

  expect(fetch).toHaveBeenCalledWith(
    "/api/clients/client-1/inventory-boot",
    expect.objectContaining({ method: "POST" }),
  );
});

it("reads driver readiness for an image", async () => {
  await getDriverReadiness("tank/image/windows");

  expect(fetch).toHaveBeenCalledWith(
    "/api/images/tank%2Fimage%2Fwindows/driver-readiness",
    expect.anything(),
  );
});
