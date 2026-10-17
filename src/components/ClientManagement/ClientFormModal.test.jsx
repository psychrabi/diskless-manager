import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { useAuth } from "@/contexts/auth";
import { addClient, deleteClientWriteback, getClientNvmeOfStatus, rotateChapSecret, updateClient } from "@/api/modules/clients";
import { prepareWindowsAssignment } from "@/api/modules/windowsServicing";
import { useAppStore } from "@/store/useAppStore";
import { listGameDisks } from "@/api/modules/zfs";
import ClientFormModal from "./ClientFormModal";

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn() }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => toast }));

vi.mock("@/contexts/auth", () => ({ useAuth: vi.fn(() => ({ user: { role: "admin" } })) }));

vi.mock("@/api/modules/clients", async (original) => ({
  ...await original(),
  addClient: vi.fn(),
  updateClient: vi.fn(),
  getClientNvmeOfStatus: vi.fn(),
  rotateChapSecret: vi.fn(),
  deleteClientWriteback: vi.fn(),
}));

vi.mock("@/api/modules/windowsServicing", () => ({
  prepareWindowsAssignment: vi.fn(),
}));

const confirm = vi.fn();
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => confirm }));

vi.mock("@/api/modules/zfs", () => ({
  listGameDisks: vi.fn().mockResolvedValue({ disks: [] }),
}));

beforeEach(() => {
  useAuth.mockReturnValue({ user: { role: "admin" } });
  confirm.mockReset();
  deleteClientWriteback.mockReset();
  addClient.mockReset();
  updateClient.mockReset();
  prepareWindowsAssignment.mockReset();
  useAppStore.setState({ appConfig: { settings: {} } });
  toast.error.mockReset();
  toast.success.mockReset();
});

it("ignores an earlier client's status response after switching clients", async () => {
  let finishFirst;
  getClientNvmeOfStatus.mockImplementation((id) => id === "one"
    ? new Promise((resolve) => { finishFirst = resolve; })
    : Promise.resolve({ nqn: "second-client", subsystem_present: true, namespace_enabled: true, port_attached: true }));
  const client = { id: "one", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", master: "windows", snapshot: "windows@ready", keep_writeback: false };
  const props = { client, masters: [], isOpen: true, onClose: () => {}, refresh: () => {} };
  const { rerender } = render(<ClientFormModal {...props} />);
  rerender(<ClientFormModal {...props} client={{ ...client, id: "two", name: "PC002" }} />);
  expect(await screen.findByText("second-client")).toBeInTheDocument();
  await act(async () => finishFirst({ nqn: "first-client", subsystem_present: false }));
  expect(screen.getByText("second-client")).toBeInTheDocument();
  expect(screen.queryByText("first-client")).not.toBeInTheDocument();
  expect(screen.getByDisplayValue("PC002")).toBeInTheDocument();
});

it("shows a link to Disks when the game switch is on but no game disks exist", async () => {
  listGameDisks.mockResolvedValue({ disks: [] });
  getClientNvmeOfStatus.mockResolvedValue(null);
  const client = { id: "three", name: "PC003", mac: "00:11:22:33:44:66", ip: "192.168.1.103", master: "windows", snapshot: "windows@ready", keep_writeback: false, use_game_disk: true, game_disks: [] };
  render(
    <MemoryRouter>
      <ClientFormModal client={client} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />
    </MemoryRouter>,
  );
  expect(await screen.findByText(/No game disks yet/)).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /create one in disks/i })).toHaveAttribute("href", "/disks");
});

it("lists discovered game disks for selection", async () => {
  listGameDisks.mockResolvedValue({
    disks: [
      { dataset: "diskless/games/steam-games", disk_type: "game", size_bytes: 53687091200, used_by: [], clones: [] },
    ],
  });
  getClientNvmeOfStatus.mockResolvedValue(null);
  const client = { id: "four", name: "PC004", mac: "00:11:22:33:44:67", ip: "192.168.1.104", master: "windows", snapshot: "windows@ready", keep_writeback: false, use_game_disk: true, game_disks: [] };
  render(
    <MemoryRouter>
      <ClientFormModal client={client} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />
    </MemoryRouter>,
  );
  expect(await screen.findByText("steam-games")).toBeInTheDocument();
  expect(screen.getByText(/50.0 GB/)).toBeInTheDocument();
});

it("shows credentials only from an administrator's rotation response", async () => {
  getClientNvmeOfStatus.mockResolvedValue(null);
  rotateChapSecret.mockResolvedValue({ username: "rotated-user", password: "rotated-secret" });
  const client = { id: "five", name: "PC005", mac: "00:11:22:33:44:68", ip: "192.168.1.105", master: "windows", chap_secret: "old-secret" };
  await act(async () => {
    render(<ClientFormModal client={client} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  });
  expect(screen.queryByText("old-secret")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret" }));
  expect(await screen.findByText("rotated-secret")).toBeInTheDocument();
  expect(screen.getByText("rotated-user")).toBeInTheDocument();
});

it("disables credential rotation for regular users", async () => {
  useAuth.mockReturnValue({ user: { role: "user" } });
  getClientNvmeOfStatus.mockResolvedValue(null);
  const client = { id: "six", name: "PC006", mac: "00:11:22:33:44:69", ip: "192.168.1.106", master: "windows" };
  await act(async () => {
    render(<ClientFormModal client={client} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  });
  expect(screen.getByRole("button", { name: "Rotate secret" })).toBeDisabled();
});

it("shows driver guidance without requiring inventory or a boot test", async () => {
  getClientNvmeOfStatus.mockResolvedValue(null);
  const masters = [
    { name: "linux", os_type: "linux", snapshots: [] },
    { name: "windows11", os_type: "windows", snapshots: [] },
  ];
  render(
    <MemoryRouter>
      <ClientFormModal client={null} masters={masters} isOpen onClose={() => {}} refresh={() => {}} />
    </MemoryRouter>,
  );
  expect(screen.queryByTestId("windows-driver-hint")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Select Image"), { target: { value: "windows11" } });
  expect(await screen.findByTestId("windows-driver-hint")).toHaveTextContent(/Boot tests do not block assignment/);
  fireEvent.change(screen.getByLabelText("Select Image"), { target: { value: "linux" } });
  expect(screen.queryByTestId("windows-driver-hint")).not.toBeInTheDocument();
});

it("prepares Windows NIC drivers before assigning a Windows image", async () => {
  getClientNvmeOfStatus.mockResolvedValue(null);
  prepareWindowsAssignment.mockResolvedValue({
    master: "windows11-serviced",
    snapshot: "windows11-serviced@serviced-1",
    prepared: true,
    reused: false,
    already_covered: false,
  });
  addClient.mockResolvedValue({ id: "new-client" });
  useAppStore.setState({
    appConfig: {
      settings: {
        windows_servicing: {
          worker_host: "192.0.2.50",
          worker_username: "Administrator",
          iscsi_portal: "192.0.2.1",
          report_server_url: "http://192.0.2.1:8080",
        },
      },
    },
  });
  render(
    <MemoryRouter>
      <ClientFormModal
        client={null}
        masters={[{ name: "windows11", os_type: "windows", snapshots: [{ name: "windows11@installed" }] }]}
        isOpen
        onClose={() => {}}
        refresh={() => {}}
      />
    </MemoryRouter>,
  );

  fireEvent.change(screen.getByLabelText("Client Name"), { target: { value: "PC011" } });
  fireEvent.change(screen.getByLabelText("MAC Address"), { target: { value: "02:00:00:00:00:11" } });
  fireEvent.change(screen.getByLabelText("IP Address"), { target: { value: "192.0.2.11" } });
  fireEvent.change(screen.getByLabelText("Select Image"), { target: { value: "windows11" } });
  fireEvent.change(screen.getByLabelText("Select Snapshot"), { target: { value: "windows11@installed" } });
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Save" }));

  await waitFor(() => expect(addClient).toHaveBeenCalled());
  await waitFor(() => expect(prepareWindowsAssignment).toHaveBeenCalledWith({
    image_name: "windows11",
    snapshot: "windows11@installed",
    mac: "02:00:00:00:00:11",
  }));
  expect(addClient).toHaveBeenCalledWith(expect.objectContaining({
    master: "windows11-serviced",
    snapshot: "windows11-serviced@serviced-1",
  }));
});

const deletableClient = { id: "writeback-1", name: "PC010", mac: "00:11:22:33:44:70", ip: "192.168.1.110", master: "windows", snapshot: "windows@ready", writeback: "diskless/clients/PC010/writeback", status: "Offline" };

it("confirms Delete Writeback and leaves storage alone when cancelled", async () => {
  confirm.mockResolvedValueOnce(false);
  getClientNvmeOfStatus.mockResolvedValue(null);
  render(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  fireEvent.click(await screen.findByRole("button", { name: "Delete Writeback" }));
  expect(confirm).toHaveBeenCalledWith(expect.objectContaining({
    title: "Delete Writeback",
    description: expect.stringMatching(/boot and game clones.*next PXE boot/),
    confirmVariant: "destructive",
  }));
  expect(deleteClientWriteback).not.toHaveBeenCalled();
});

it("deletes writeback, reports success, and refreshes clients", async () => {
  confirm.mockResolvedValueOnce(true);
  deleteClientWriteback.mockResolvedValueOnce({ message: "Deleted" });
  getClientNvmeOfStatus.mockResolvedValue(null);
  const refresh = vi.fn();
  render(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={() => {}} refresh={refresh} />);
  fireEvent.click(await screen.findByRole("button", { name: "Delete Writeback" }));
  await screen.findByRole("button", { name: "Delete Writeback" });
  expect(screen.getByRole("button", { name: "Delete Writeback" })).toBeInTheDocument();
  expect(deleteClientWriteback).toHaveBeenCalledWith("writeback-1");
  expect(refresh).toHaveBeenCalledOnce();
});

it("shows a server error without refreshing after deletion fails", async () => {
  const failure = new Error("Client is online");
  confirm.mockResolvedValueOnce(true);
  deleteClientWriteback.mockRejectedValueOnce(failure);
  getClientNvmeOfStatus.mockResolvedValue(null);
  const refresh = vi.fn();
  render(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={() => {}} refresh={refresh} />);
  fireEvent.click(await screen.findByRole("button", { name: "Delete Writeback" }));
  await screen.findByRole("button", { name: "Delete Writeback" });
  expect(refresh).not.toHaveBeenCalled();
  expect(toast.error).toHaveBeenCalledWith("Client Management", failure);
});

it("keeps the dialog open when Escape is pressed during deletion", async () => {
  let finishDeletion;
  confirm.mockResolvedValueOnce(true);
  deleteClientWriteback.mockReturnValueOnce(new Promise((resolve) => { finishDeletion = resolve; }));
  getClientNvmeOfStatus.mockResolvedValue(null);
  const onClose = vi.fn();
  render(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={onClose} refresh={() => {}} />);
  fireEvent.click(await screen.findByRole("button", { name: "Delete Writeback" }));
  await vi.waitFor(() => expect(deleteClientWriteback).toHaveBeenCalledWith("writeback-1"));

  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.click(document.querySelector('[data-slot="dialog-overlay"]'));
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(onClose).not.toHaveBeenCalled();

  await act(async () => finishDeletion({ message: "Deleted" }));
});

it("limits Delete Writeback to admins and configured offline clients", async () => {
  getClientNvmeOfStatus.mockResolvedValue(null);
  const { rerender } = render(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  expect(await screen.findByRole("button", { name: "Delete Writeback" })).toBeEnabled();
  rerender(<ClientFormModal client={{ ...deletableClient, status: "Online" }} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  expect(screen.getByRole("button", { name: "Delete Writeback" })).toBeDisabled();
  useAuth.mockReturnValue({ user: { role: "user" } });
  rerender(<ClientFormModal client={deletableClient} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  expect(screen.queryByRole("button", { name: "Delete Writeback" })).not.toBeInTheDocument();
  useAuth.mockReturnValue({ user: { role: "admin" } });
  rerender(<ClientFormModal client={{ ...deletableClient, writeback: null }} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  expect(screen.queryByRole("button", { name: "Delete Writeback" })).not.toBeInTheDocument();
  rerender(<ClientFormModal client={null} masters={[]} isOpen onClose={() => {}} refresh={() => {}} />);
  expect(screen.queryByRole("button", { name: "Delete Writeback" })).not.toBeInTheDocument();
});
