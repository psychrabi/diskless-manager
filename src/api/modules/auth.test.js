import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  bootstrapAdmin,
  checkAdminExists,
  updateAdminPassword,
} from "./auth";
import { apiRequest, setAuthToken } from "../client";

function response(body, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 409 ? "Conflict" : "OK",
    headers: { get: () => "application/json" },
    json: vi.fn().mockResolvedValue(body),
    text: vi.fn().mockResolvedValue(JSON.stringify(body)),
  };
}

describe("authentication API contracts", () => {
  beforeEach(() => {
    setAuthToken(null);
    localStorage.clear();
    globalThis.fetch = vi.fn();
  });

  it("creates the chosen first administrator without a default password", async () => {
    fetch.mockResolvedValue(response({}, 201));

    await bootstrapAdmin("operator", "StrongPass1");

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8080/api/auth/bootstrap",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ username: "operator", password: "StrongPass1" }),
      })
    );
  });

  it("sends administrator password changes with authentication", async () => {
    setAuthToken("signed-token");
    fetch.mockResolvedValue(response({ message: "updated" }));

    await updateAdminPassword({
      old_password: "OldPass1",
      new_password: "NewPass2",
    });

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8080/api/auth/admin/password",
      expect.objectContaining({
        method: "PUT",
        headers: expect.objectContaining({ Authorization: "Bearer signed-token" }),
      })
    );
  });

  it("keeps existing installations out of first-run setup", async () => {
    fetch.mockResolvedValue(response({ exists: true }));

    await expect(checkAdminExists()).resolves.toEqual({ exists: true });
  });

  it("surfaces the message from the structured API error contract", async () => {
    fetch.mockResolvedValue(
      response(
        {
          code: "state_conflict",
          message: "Client has an active session",
          operation_id: "operation-123",
          details: {},
        },
        409
      )
    );

    await expect(apiRequest("/api/clients/client-1")).rejects.toThrow(
      "Client has an active session"
    );
  });
});
