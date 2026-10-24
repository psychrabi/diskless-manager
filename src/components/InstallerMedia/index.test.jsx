import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { api, toast } = vi.hoisted(() => ({
  api: { list: vi.fn(), upload: vi.fn(), setDefault: vi.fn(), remove: vi.fn(), settings: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("@/api/modules/installerMedia", () => ({
  listInstallerMedia: api.list,
  uploadInstallerMedia: api.upload,
  setDefaultInstallerMedia: api.setDefault,
  deleteInstallerMedia: api.remove,
}));
vi.mock("@/api/modules/system", () => ({ getSettings: api.settings }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => toast }));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => vi.fn().mockResolvedValue(true) }));

import InstallerMedia from "./index";

const item = {
  id: "media-1", display_name: "WinPE test", media_type: "winpe", byte_size: 2048,
  checksum: "sha256-value", validation_state: "valid", is_default: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  api.list.mockResolvedValue([item]);
  api.settings.mockResolvedValue({ installer_media: { max_upload_bytes: 10 * 1024 * 1024 } });
  api.upload.mockResolvedValue({ ...item, display_name: "Uploaded WinPE" });
  api.setDefault.mockResolvedValue("");
  api.remove.mockResolvedValue("");
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
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Installer media", "Catalog unavailable"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
    expect(toast.success).toHaveBeenCalledWith("Installer media", "Installer ISO uploaded and validated.");
  });

  it("sets a default for the selected media type", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    await user.click(within(article).getByRole("button", { name: "Set as default" }));
    expect(api.setDefault).toHaveBeenCalledWith("media-1");
    expect(toast.success).toHaveBeenCalledWith("Installer media", "WinPE test is now the default for its type.");
  });

  it("reports a referenced-media delete refusal", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    await screen.findByRole("article");
    api.remove.mockRejectedValueOnce(new Error("Media is selected or in use"));
    await user.click(within(await screen.findByRole("article")).getByRole("button", { name: "Delete" }));
    expect(api.remove).toHaveBeenCalledWith("media-1");
    expect(toast.error).toHaveBeenCalledWith("Installer media", "Media is selected or in use");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the ISO catalog without client boot-verification controls", async () => {
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    expect(within(article).getByText("Default: No")).toBeInTheDocument();
    expect(within(article).queryByLabelText("Test client")).not.toBeInTheDocument();
    expect(within(article).queryByLabelText("Boot result")).not.toBeInTheDocument();
    expect(within(article).queryByText(/Observed LUNs/)).not.toBeInTheDocument();
  });
});
