import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";

const { api, store, toast, confirm } = vi.hoisted(() => ({
  api: {
    list: vi.fn(),
    start: vi.fn(),
    cancel: vi.fn(),
    closeRecovery: vi.fn(),
    identity: vi.fn(),
    capture: vi.fn(),
    apply: vi.fn(),
    remove: vi.fn(),
    inventory: vi.fn(),
  },
  store: { clients: [] },
  toast: { success: vi.fn(), error: vi.fn() },
  confirm: vi.fn(),
}));

vi.mock("@/api/modules/clientProvisioning", () => ({
  listInstallations: api.list,
  startInstallation: api.start,
  cancelInstallation: api.cancel,
  closeInstallationRecovery: api.closeRecovery,
  getHardwareIdentity: api.identity,
  captureHardwareIdentity: api.capture,
  applyHardwareIdentity: api.apply,
  removeHardwareIdentity: api.remove,
  requestHardwareInventory: api.inventory,
}));
// A stable fetchClients identity keeps the mount effect from re-running each render.
vi.mock("@/store/useAppStore", () => {
  const fetchClients = vi.fn().mockResolvedValue([]);
  return { useAppStore: (selector) => selector({ clients: store.clients, fetchClients }) };
});
vi.mock("@/store/useToastStore", () => {
  const useToastStore = () => toast;
  useToastStore.getState = () => toast;
  return { useToastStore };
});
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => confirm }));

import ClientProvisioning from "./index";

const client = {
  id: "client-1",
  name: "PC001",
  ip: "192.0.2.10",
  mac: "02:00:00:00:00:01",
  master: "pool/windows",
  pxe_mode: "uefi",
  status: "Offline",
  installer_media_id: "media-1",
};
const passwordHash = `$6$salt$${"a".repeat(86)}`;

function installation(state, progress = 40) {
  return {
    session: {
      id: `session-${state}`,
      recipe: "interactive",
      state,
      progress,
      error: null,
    },
    events: [],
  };
}

function renderPage() {
  return render(
    <MemoryRouter>
      <ClientProvisioning />
    </MemoryRouter>,
  );
}

async function selectClient(user) {
  await user.selectOptions(screen.getByLabelText("Client"), client.id);
  return screen.findByText(/Assigned image: pool\/windows/);
}

beforeEach(() => {
  vi.clearAllMocks();
  store.clients = [client];
  confirm.mockResolvedValue(true);
  api.list.mockResolvedValue([]);
  api.identity.mockResolvedValue({ profile: null, cache: null });
  api.start.mockResolvedValue({ id: "session-ready", state: "ready", progress: 0 });
  api.cancel.mockResolvedValue({ status: "cancelled" });
  api.closeRecovery.mockResolvedValue({ status: "completed" });
});

afterEach(() => vi.restoreAllMocks());

describe("Client provisioning page", () => {
  it("prepares an unattended installation only after confirmed credentials", async () => {
    const user = userEvent.setup();
    renderPage();
    await selectClient(user);

    const method = screen.getByLabelText("Installation method");
    const prepare = screen.getByRole("button", { name: "Prepare installation" });
    await waitFor(() => expect(prepare).toBeEnabled());

    // Unattended Linux needs an explicit username and a password hash.
    await user.selectOptions(method, "ubuntu");
    expect(prepare).toBeDisabled();
    await user.type(screen.getByLabelText("Linux username"), "operator");
    await user.type(screen.getByLabelText("SHA-512 password hash"), passwordHash);
    expect(prepare).toBeEnabled();

    // Declining the erase confirmation sends nothing.
    confirm.mockResolvedValueOnce(false);
    await user.click(prepare);
    expect(api.start).not.toHaveBeenCalled();

    await user.click(prepare);
    await waitFor(() =>
      expect(api.start).toHaveBeenCalledWith("client-1", {
        recipe: "ubuntu",
        confirm_erase: true,
        username: "operator",
        password_hash: passwordHash,
      }),
    );
    expect(confirm).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Prepare client installation",
        confirmVariant: "destructive",
      }),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Client provisioning",
        "Installation prepared. Boot the selected client to continue.",
      ),
    );
  });

  it("cancels a running session and leaves recovery to a separate confirmation", async () => {
    const user = userEvent.setup();
    api.list.mockResolvedValue([installation("installing")]);
    renderPage();
    await selectClient(user);

    const cancel = await screen.findByRole("button", { name: "Cancel session" });
    expect(screen.getByText(/Interactive ISO boot: installing · 40%/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clear recovery" })).not.toBeInTheDocument();

    await user.click(cancel);
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith("session-installing"));
    expect(api.closeRecovery).not.toHaveBeenCalled();
    expect(confirm).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Cancel installation session",
        confirmVariant: "destructive",
      }),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Client provisioning",
        "New installer requests stopped. Shut down the client before clearing recovery.",
      ),
    );
  });

  it("clears recovery for a stopped session without offering cancel", async () => {
    const user = userEvent.setup();
    api.list.mockResolvedValue([installation("cancelled", 30)]);
    renderPage();
    await selectClient(user);

    const clear = await screen.findByRole("button", { name: "Clear recovery" });
    expect(screen.queryByRole("button", { name: "Cancel session" })).not.toBeInTheDocument();

    await user.click(clear);
    await waitFor(() => expect(api.closeRecovery).toHaveBeenCalledWith("session-cancelled"));
    expect(api.cancel).not.toHaveBeenCalled();
    expect(confirm).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Close installation recovery" }),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Client provisioning", "Installation recovery closed."),
    );
  });

  it("shows recorded session errors and its events", async () => {
    const user = userEvent.setup();
    api.list.mockResolvedValue([
      {
        session: {
          id: "session-failed",
          recipe: "rhel",
          state: "failed",
          progress: 10,
          error: "RHEL 9 x86_64 full DVD is required",
        },
        events: [
          { id: 2, state: "failed", message: "Installer failed; inspect console", created_at: "2026-10-25T08:00:00Z" },
        ],
      },
    ]);
    renderPage();
    await selectClient(user);

    const article = await screen.findByRole("article");
    expect(within(article).getByText("RHEL 9 x86_64 full DVD is required")).toBeInTheDocument();
    expect(within(article).getByText(/Installer failed; inspect console/)).toBeInTheDocument();
    expect(within(article).getByRole("button", { name: "Clear recovery" })).toBeInTheDocument();
  });
});
