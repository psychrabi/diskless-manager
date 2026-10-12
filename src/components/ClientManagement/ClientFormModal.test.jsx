import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { useAuth } from "@/contexts/auth";
import { getClientNvmeOfStatus, rotateChapSecret } from "@/api/modules/clients";
import { listGameDisks } from "@/api/modules/zfs";
import ClientFormModal from "./ClientFormModal";

vi.mock("@/contexts/auth", () => ({ useAuth: vi.fn(() => ({ user: { role: "admin" } })) }));

vi.mock("@/api/modules/clients", async (original) => ({
  ...await original(),
  getClientNvmeOfStatus: vi.fn(),
  rotateChapSecret: vi.fn(),
}));

vi.mock("@/api/modules/zfs", () => ({
  listGameDisks: vi.fn().mockResolvedValue({ disks: [] }),
}));

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

it("explains the inventory requirement for a new Windows client", async () => {
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
  expect(await screen.findByTestId("windows-driver-hint")).toHaveTextContent(/preboot NIC inventory/);
  fireEvent.change(screen.getByLabelText("Select Image"), { target: { value: "linux" } });
  expect(screen.queryByTestId("windows-driver-hint")).not.toBeInTheDocument();
});
