import { beforeEach, expect, it, vi } from "vitest";

const { apiRequest } = vi.hoisted(() => ({ apiRequest: vi.fn() }));
vi.mock("../client", () => ({ apiRequest }));

import { discardSuper, enterSuper, exitLegacySuper, getSuperStatus, saveSuper } from "./clients";

beforeEach(() => apiRequest.mockReset());

it("exits legacy Super using the selected master snapshot", async () => {
  apiRequest.mockResolvedValue({ message: "Legacy Super exited" });

  await exitLegacySuper("client/1", "pool/windows@ready");

  expect(apiRequest).toHaveBeenCalledWith(
    "/api/clients/client%2F1/legacy-super/exit",
    { method: "POST", body: JSON.stringify({ snapshot: "pool/windows@ready" }) },
  );
});

it("uses the managed Super session endpoints", async () => {
  await getSuperStatus("client/1");
  await enterSuper("client/1", { verification_only: true, confirmed: true });
  await saveSuper("client/1");
  await discardSuper("client/1");

  expect(apiRequest.mock.calls).toEqual([
    ["/api/clients/client%2F1/super/status"],
    ["/api/clients/client%2F1/super/enter", { method: "POST", body: JSON.stringify({ verification_only: true, confirmed: true }) }],
    ["/api/clients/client%2F1/super/save", { method: "POST" }],
    ["/api/clients/client%2F1/super/discard", { method: "POST" }],
  ]);
});
