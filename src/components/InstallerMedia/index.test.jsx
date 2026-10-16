import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { api, state } = vi.hoisted(() => ({
  api: { list: vi.fn(), upload: vi.fn(), setDefault: vi.fn(), remove: vi.fn(), verify: vi.fn(), settings: vi.fn(), superStatus: vi.fn() },
  state: { clients: [] },
}));
vi.mock("@/api/modules/installerMedia", () => ({
  listInstallerMedia: api.list,
  uploadInstallerMedia: api.upload,
  setDefaultInstallerMedia: api.setDefault,
  deleteInstallerMedia: api.remove,
  recordInstallerMediaVerification: api.verify,
}));
vi.mock("@/api/modules/system", () => ({ getSettings: api.settings }));
vi.mock("@/api/modules/clients", () => ({ getSuperStatus: api.superStatus }));
vi.mock("@/store/useAppStore", () => ({
  useAppStore: (selector) => selector({ clients: state.clients, fetchClients: vi.fn().mockResolvedValue(state.clients) }),
}));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ success: vi.fn(), error: vi.fn() }) }));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => vi.fn().mockResolvedValue(true) }));

import InstallerMedia from "./index";

const item = {
  id: "media-1", display_name: "WinPE test", media_type: "winpe", byte_size: 2048,
  checksum: "sha256-value", validation_state: "valid", is_default: false,
  bios_verified: true, uefi_verified: false,
};
const client = { id: "client-1", name: "Test PC", master: "pool/windows", snapshot: "pool/windows@rev-1", pxe_mode: "uefi" };

beforeEach(() => {
  vi.clearAllMocks();
  state.clients = [client];
  api.list.mockResolvedValue([item]);
  api.settings.mockResolvedValue({ installer_media: { max_upload_bytes: 10 * 1024 * 1024 } });
  api.superStatus.mockResolvedValue({ session: null });
  api.upload.mockResolvedValue({ ...item, display_name: "Uploaded WinPE" });
  api.setDefault.mockResolvedValue("");
  api.remove.mockResolvedValue("");
  api.verify.mockResolvedValue("");
});

afterEach(() => vi.restoreAllMocks());

describe("Installer Media page", () => {
  it("shows loading and load errors", async () => {
    let finish;
    api.list.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    render(<InstallerMedia />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading installer media");
    finish([item]);
    expect(await screen.findByText("WinPE test")).toBeInTheDocument();

    api.list.mockRejectedValueOnce(new Error("Catalog unavailable"));
    render(<InstallerMedia />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Catalog unavailable");
  });

  it("uploads the chosen media type and displays the upload result", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    await screen.findByText("WinPE test");
    await user.type(screen.getByLabelText("Display name"), "New Linux ISO");
    await user.selectOptions(screen.getByLabelText("Media type"), "linux_live");
    const file = new File(["iso"], "linux.iso", { type: "application/octet-stream" });
    await user.upload(screen.getByLabelText("ISO file"), file);
    expect(screen.getByLabelText("ISO file").files[0]).toBe(file);
    await user.click(screen.getByRole("button", { name: "Upload and validate" }));
    await waitFor(() => expect(api.upload).toHaveBeenCalledWith({ displayName: "New Linux ISO", mediaType: "linux_live", file }));
    expect(await screen.findByRole("status")).toHaveTextContent("uploaded and validated");
  });

  it("sets a default for the selected media type", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    await user.click(within(article).getByRole("button", { name: "Set as default" }));
    expect(api.setDefault).toHaveBeenCalledWith("media-1");
    await screen.findByText("WinPE test is now the default for its type.");
  });

  it("reports a referenced-media delete refusal", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    await screen.findByRole("article");
    api.remove.mockRejectedValueOnce(new Error("Media is selected or in use"));
    await user.click(within(await screen.findByRole("article")).getByRole("button", { name: "Delete" }));
    expect(api.remove).toHaveBeenCalledWith("media-1");
    expect(await screen.findByRole("alert")).toHaveTextContent("Media is selected or in use");
  });

  it("shows firmware verification status and records the selected client result", async () => {
    const user = userEvent.setup();
    api.superStatus.mockResolvedValue({ session: { state: "active", verification_only: true, installer_media_id: "media-1" } });
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    expect(within(article).getByText(/BIOS: Verified · UEFI: Not verified/)).toBeInTheDocument();
    await user.selectOptions(within(article).getByLabelText("Test client"), "client-1");
    await waitFor(() => expect(within(article).getByRole("button", { name: "Record observed boot result" })).toBeEnabled());
    await user.click(within(await screen.findByRole("article")).getByRole("button", { name: "Record observed boot result" }));
    await waitFor(() => expect(api.verify).toHaveBeenCalledWith("media-1", {
      checksum: "sha256-value", firmware_mode: "uefi", test_client_id: "client-1",
      assigned_image_revision: "pool/windows@rev-1", observed_luns: [0, 1], boot_result: "passed",
    }));
    expect(await screen.findByRole("status")).toHaveTextContent("UEFI boot result recorded");
  });

  it("does not offer recording for an ISO different from the active verification session", async () => {
    const user = userEvent.setup();
    api.superStatus.mockResolvedValue({ session: { state: "active", verification_only: true, installer_media_id: "other-media" } });
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    await user.selectOptions(within(article).getByLabelText("Test client"), "client-1");
    expect(await screen.findByText(/active verification session is using a different ISO/i)).toBeInTheDocument();
    expect(within(article).getByRole("button", { name: "Record observed boot result" })).toBeDisabled();
  });
});
