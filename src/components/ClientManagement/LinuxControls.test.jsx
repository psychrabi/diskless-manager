import { fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import PowerActionModal from "./PowerActionModal";
import RemoteDesktopModal from "./RemoteDesktopModal";
import { rebootClient, remoteDesktopClient } from "@/api/modules/control";
const state = vi.hoisted(() => ({ masters: [], success: vi.fn(), error: vi.fn() }));
vi.mock("@/store/useAppStore", () => ({ useAppStore: (selector) => selector(state) }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => state }));
vi.mock("@/api/modules/control", () => ({ rebootClient: vi.fn(), shutdownClient: vi.fn(), remoteDesktopClient: vi.fn() }));
const client = { id: "pc001", name: "PC001", master: "diskless/image-disk/fedora-fixed" };
beforeEach(() => { vi.clearAllMocks(); state.masters = []; });
afterEach(cleanup);

it("sends the selected SSH user for Linux restart", async () => {
  rebootClient.mockResolvedValue({ success: true });
  render(<PowerActionModal client={client} isOpen type="reboot" onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("SSH username"), { target: { value: "admin" } });
  fireEvent.click(screen.getByRole("button", { name: "Reboot" }));
  await waitFor(() => expect(rebootClient).toHaveBeenCalledWith("pc001", { force: false, delay_minutes: null, ssh_username: "admin" }));
});

it("uses Linux image metadata over a Windows-looking name", () => {
  const custom = { ...client, master: "windows-looking" };
  state.masters = [{ name: custom.master, os_type: "linux" }];
  render(<RemoteDesktopModal client={custom} isOpen onClose={() => {}} />);
  expect(screen.getByLabelText("Username")).toHaveValue("diskless");
  expect(screen.getByLabelText("Password")).toHaveValue("");
});

it("uses an alternate RDP port and shows launch errors without a success toast", async () => {
  remoteDesktopClient.mockResolvedValue({ success: false, message: "Viewer unavailable" });
  render(<RemoteDesktopModal client={client} isOpen onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "test-password" } });
  fireEvent.change(screen.getByLabelText("RDP port"), { target: { value: "3390" } });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() => expect(remoteDesktopClient).toHaveBeenCalledWith("pc001", { username: "diskless", password: "test-password", port: 3390 }));
  expect(state.error).toHaveBeenCalledWith("Remote Desktop", "Failed to connect: Viewer unavailable");
  expect(state.success).not.toHaveBeenCalled();
});

it("clears credentials and restores the Windows user when switching clients", () => {
  const props = { isOpen: true, onClose: () => {} };
  const { rerender } = render(<RemoteDesktopModal {...props} client={client} />);
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "secret" } });
  rerender(<RemoteDesktopModal {...props} client={{ ...client, id: "other", master: "windows11" }} />);
  expect(screen.getByLabelText("Username")).toHaveValue("Administrator");
  expect(screen.getByLabelText("Password")).toHaveValue("");
});
