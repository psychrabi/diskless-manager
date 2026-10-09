import { beforeEach, expect, it, vi } from "vitest";
import { apiRequest } from "../client";
import { rebootClient, shutdownClient } from "./control";
vi.mock("../client", () => ({ apiRequest: vi.fn().mockResolvedValue({ success: true }) }));
beforeEach(() => vi.clearAllMocks());
it.each([rebootClient, shutdownClient])("passes the selected SSH user to power controls", async (action) => {
  await action("pc001", { ssh_username: "admin", delay_minutes: 5 });
  expect(JSON.parse(apiRequest.mock.calls[0][1].body)).toEqual({ force: false, delay_minutes: 5, ssh_username: "admin" });
});
