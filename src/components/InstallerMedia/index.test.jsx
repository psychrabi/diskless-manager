import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { api, toast } = vi.hoisted(() => ({
  api: {
    list: vi.fn(), upload: vi.fn(), setDefault: vi.fn(), remove: vi.fn(), settings: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("@/api/modules/installerMedia", () => ({
  listInstallerMedia: api.list,
  uploadInstallerMedia: api.upload,
  setDefaultInstallerMedia: api.setDefault,
  deleteInstallerMedia: api.remove,
}));
vi.mock("@/api/modules/system", () => ({ getSettings: api.settings }));
vi.mock("@/store/useToastStore", () => {
  const useToastStore = () => toast;
  useToastStore.getState = () => toast;
  return { useToastStore };
});
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => vi.fn().mockResolvedValue(true) }));

import { useInstallerMediaUploadStore } from "@/store/useInstallerMediaUploadStore";
import InstallerMedia from "./index";

const item = {
  id: "media-1", display_name: "WinPE test", media_type: "winpe", byte_size: 2048,
  checksum: "sha256-value", validation_state: "valid", is_default: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  useInstallerMediaUploadStore.getState().reset();
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

  it("offers exactly the four supported media types", async () => {
    render(<InstallerMedia />);
    await screen.findByText("WinPE test");
    const options = within(screen.getByLabelText("Media type"))
      .getAllByRole("option")
      .map((option) => option.textContent);
    expect(options).toEqual(["Windows PE", "Windows ISO", "Linux ISO", "Other ISO"]);
  });

  it("keeps the installer catalog free of client test and boot-result controls", async () => {
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    expect(within(article).getByText("Default: No")).toBeInTheDocument();
    expect(within(article).queryByLabelText("Test client")).not.toBeInTheDocument();
    expect(within(article).queryByLabelText("Boot result")).not.toBeInTheDocument();
    expect(within(article).queryByRole("button", { name: "Record observed boot result" })).not.toBeInTheDocument();
  });

  it("uploads the chosen media type and displays the upload result", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    await screen.findByText("WinPE test");
    await user.type(screen.getByLabelText("Display name"), "New Linux ISO");
    await user.selectOptions(screen.getByLabelText("Media type"), "linux_iso");
    const file = new File(["iso"], "linux.iso", { type: "application/octet-stream" });
    await user.upload(screen.getByLabelText("ISO file"), file);
    expect(screen.getByLabelText("ISO file").files[0]).toBe(file);
    await user.click(screen.getByRole("button", { name: "Upload and validate" }));
    await waitFor(() => expect(api.upload).toHaveBeenCalledWith({
      displayName: "New Linux ISO",
      mediaType: "linux_iso",
      file,
      onProgress: expect.any(Function),
    }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("Installer media", "New Linux ISO uploaded and validated."));
  });

  it("shows live upload and validation progress", async () => {
    let reportProgress;
    let resolveUpload;
    api.upload.mockImplementation(({ onProgress }) => new Promise((resolve) => {
      reportProgress = onProgress;
      resolveUpload = resolve;
    }));

    const user = userEvent.setup();
    render(<InstallerMedia />);
    await screen.findByText("WinPE test");
    await user.type(screen.getByLabelText("Display name"), "New Linux ISO");
    const file = new File(["iso"], "linux.iso", { type: "application/octet-stream" });
    await user.upload(screen.getByLabelText("ISO file"), file);
    await user.click(screen.getByRole("button", { name: "Upload and validate" }));

    const progress = await screen.findByRole("progressbar");
    expect(progress).toHaveAttribute("aria-valuenow", "0");
    expect(screen.getByText("Uploading linux.iso…")).toBeInTheDocument();

    act(() => reportProgress(0.42));
    expect(screen.getByText("Uploading linux.iso…")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");

    act(() => reportProgress(1));
    expect(screen.getByText("Validating linux.iso…")).toBeInTheDocument();

    await act(async () => resolveUpload({ ...item }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("Installer media", "New Linux ISO uploaded and validated."));
  });

  it("continues uploading after the page unmounts and notifies when complete", async () => {
    let resolveUpload;
    api.upload.mockImplementation(() => new Promise((resolve) => {
      resolveUpload = resolve;
    }));

    const user = userEvent.setup();
    const { unmount } = render(<InstallerMedia />);
    await screen.findByText("WinPE test");
    await user.type(screen.getByLabelText("Display name"), "Background ISO");
    const file = new File(["iso"], "background.iso", { type: "application/octet-stream" });
    await user.upload(screen.getByLabelText("ISO file"), file);
    await user.click(screen.getByRole("button", { name: "Upload and validate" }));
    await waitFor(() => expect(api.upload).toHaveBeenCalled());

    unmount();
    expect(useInstallerMediaUploadStore.getState()).toMatchObject({ status: "uploading", displayName: "Background ISO" });
    expect(toast.success).not.toHaveBeenCalled();

    render(<InstallerMedia />);
    expect(await screen.findByText("Uploading background.iso…")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "0");

    await act(async () => resolveUpload({ ...item, display_name: "Background ISO" }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("Installer media", "Background ISO uploaded and validated."));
    expect(useInstallerMediaUploadStore.getState()).toMatchObject({ status: "success", progress: 100 });
    expect(await screen.findByText("Background ISO uploaded and validated.")).toBeInTheDocument();
  });

  it("sets the single global default", async () => {
    const user = userEvent.setup();
    render(<InstallerMedia />);
    const article = await screen.findByRole("article");
    await user.click(within(article).getByRole("button", { name: "Set as default" }));
    expect(api.setDefault).toHaveBeenCalledWith("media-1");
    expect(toast.success).toHaveBeenCalledWith("Installer media", "WinPE test is now the default installer ISO.");
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

});
