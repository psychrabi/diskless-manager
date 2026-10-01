import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { setAuthToken } from "../client";
import { downloadBackup } from "./backup";

beforeEach(() => {
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn());
  URL.createObjectURL = vi.fn().mockReturnValue("blob:backup");
  URL.revokeObjectURL = vi.fn();
});

afterEach(() => vi.restoreAllMocks());

it("downloads the untouched backup using its attachment filename and admin token", async () => {
  const content = '{"authentication_secret":"keep-me"}';
  fetch.mockResolvedValue(new Response(content, { headers: { "Content-Type": "application/json", "Content-Disposition": 'attachment; filename="diskless-backup.json"' } }));
  let downloaded;
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function () {
    downloaded = { filename: this.download, href: this.href };
  });
  await downloadBackup();
  expect(fetch).toHaveBeenCalledWith("/api/system/backup", expect.objectContaining({ headers: { Authorization: "Bearer admin-token" } }));
  expect(downloaded).toEqual({ filename: "diskless-backup.json", href: "blob:backup" });
  expect(await URL.createObjectURL.mock.calls[0][0].text()).toBe(content);
  expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:backup");
});

it.each([
  ['{"message":"Administrator required"}', "Administrator required"],
  ['broken json', "API request failed: 403 Forbidden"],
])("preserves download HTTP failure even when its JSON cannot be parsed", async (body, message) => {
  fetch.mockResolvedValue(new Response(body, { status: 403, statusText: "Forbidden", headers: { "Content-Type": "application/json" } }));
  await expect(downloadBackup()).rejects.toMatchObject({ message, status: 403 });
  expect(URL.createObjectURL).not.toHaveBeenCalled();
});
