import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { AuthContext } from "@/contexts/auth";
import { ConfirmDialogProvider } from "@/contexts/ConfirmDialogContext";
import { setAuthToken } from "@/api/client";
import BackupManagement from "./BackupManagement";
import ApplicationSettings from "./index";

function renderBackup(role = "admin", settings = false) {
  return render(
    <AuthContext.Provider value={{ user: { role } }}>
      <ConfirmDialogProvider>{settings ? <ApplicationSettings /> : <BackupManagement />}</ConfirmDialogProvider>
    </AuthContext.Provider>,
  );
}

beforeEach(() => {
  setAuthToken("admin-token");
  vi.stubGlobal("fetch", vi.fn());
});

it("offers backup and restore on the application settings page", async () => {
  fetch.mockResolvedValue(new Response('{"authorized":true}', { headers: { "Content-Type": "application/json" } }));
  renderBackup("admin", true);
  expect(screen.getByRole("button", { name: "Download backup" })).toBeInTheDocument();
  expect(screen.getByLabelText("Backup file")).toBeInTheDocument();
  await screen.findByRole("button", { name: "Already Authorized" });
});

it("hides backup controls from non-administrators", () => {
  renderBackup("user");
  expect(screen.queryByRole("button", { name: "Download backup" })).not.toBeInTheDocument();
});

it("requires a file and allows cancelling restore without uploading", async () => {
  const user = userEvent.setup();
  renderBackup();
  expect(screen.getByRole("button", { name: "Restore on restart" })).toBeDisabled();
  await user.upload(screen.getByLabelText("Backup file"), new File(['{}'], "backup.json", { type: "application/json" }));
  await user.click(screen.getByRole("button", { name: "Restore on restart" }));
  expect(screen.getByRole("dialog")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(fetch).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Restore on restart" })).toBeEnabled();
});

it("uploads the original file after confirmation and keeps restart instructions visible", async () => {
  const user = userEvent.setup();
  let complete;
  fetch.mockImplementation(() => new Promise((resolve) => { complete = resolve; }));
  renderBackup();
  const file = new File(['{"format":"backup"}'], "backup.json", { type: "application/json" });
  await user.upload(screen.getByLabelText("Backup file"), file);
  await user.click(screen.getByRole("button", { name: "Restore on restart" }));
  await user.click(screen.getByRole("button", { name: "Stage restore" }));
  expect(screen.getByRole("button", { name: "Download backup" })).toBeDisabled();
  expect(screen.getByLabelText("Backup file")).toBeDisabled();
  expect(fetch).toHaveBeenCalledWith("/api/system/restore", expect.objectContaining({
    method: "POST", body: file,
    headers: expect.objectContaining({ Authorization: "Bearer admin-token", "Content-Type": "application/json" }),
  }));
  complete(new Response(JSON.stringify({ message: "Restore staged", restart_required: true, safety_backup: "/backup/safety.json" }), { headers: { "Content-Type": "application/json" } }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Restore staged/));
  expect(screen.getByRole("status")).toHaveTextContent(/restart.*backend/i);
  expect(screen.getByRole("status")).toHaveTextContent(/original.*account/i);
  expect(screen.getByRole("status")).toHaveTextContent("/backup/safety.json");
  await waitFor(() => expect(screen.getByRole("button", { name: "Download backup" })).toBeEnabled());
});

it("keeps server validation errors visible and lets administrators retry", async () => {
  const user = userEvent.setup();
  fetch.mockResolvedValue(new Response(JSON.stringify({ message: "Invalid backup version" }), { status: 400, headers: { "Content-Type": "application/json" } }));
  renderBackup();
  await user.upload(screen.getByLabelText("Backup file"), new File(['{}'], "backup.json", { type: "application/json" }));
  await user.click(screen.getByRole("button", { name: "Restore on restart" }));
  await user.click(screen.getByRole("button", { name: "Stage restore" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Invalid backup version");
  expect(screen.getByRole("button", { name: "Restore on restart" })).toBeEnabled();
});
