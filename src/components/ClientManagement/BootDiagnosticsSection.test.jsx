import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { getClientBootDiagnostics } from "@/api/modules/clients";
import BootDiagnosticsSection from "./BootDiagnosticsSection";

vi.mock("@/api/modules/clients", () => ({ getClientBootDiagnostics: vi.fn() }));

const diagnostics = {
  read_only: true,
  client_id: "pc001",
  name: "PC001",
  firmware: "uefi",
  assigned_image: "pool/windows",
  os_type: "windows",
  target_iqn: "iqn.2026-01.client:pc001",
  initiator_iqn: "iqn.2026-01.client:client.pc001",
  keep_san: true,
  chap_enabled: false,
  ibft_gateway: "0.0.0.0",
  boot_nic_ids: ["PCI\\VEN_8086&DEV_1533"],
  hardware_architecture: "x64",
  hardware_firmware: "uefi",
  handoff: {
    attempt_id: "attempt-7",
    phase: "script_served",
    age_seconds: 42,
    expires_in_seconds: 258,
    active: true,
  },
};

beforeEach(() => {
  getClientBootDiagnostics.mockReset();
});

it("shows the gateway, target and live handoff", async () => {
  getClientBootDiagnostics.mockResolvedValue(diagnostics);
  render(<BootDiagnosticsSection clientId="pc001" />);

  expect(await screen.findByText("0.0.0.0")).toBeInTheDocument();
  expect(screen.getByText(/overrides the dhcp gateway/i)).toBeInTheDocument();
  expect(screen.getByText("iqn.2026-01.client:pc001")).toBeInTheDocument();
  expect(screen.getByText("iqn.2026-01.client:client.pc001")).toBeInTheDocument();
  expect(screen.getByText("Boot script served")).toBeInTheDocument();
  expect(screen.getByText("attempt-7")).toBeInTheDocument();
});

it("recovers after a failed load when refreshed", async () => {
  const user = userEvent.setup();
  getClientBootDiagnostics
    .mockRejectedValueOnce(new Error("Server unavailable"))
    .mockResolvedValueOnce({ ...diagnostics, ibft_gateway: "dhcp", handoff: null });
  render(<BootDiagnosticsSection clientId="pc001" />);

  expect(await screen.findByRole("alert")).toHaveTextContent("Server unavailable");
  await user.click(screen.getByRole("button", { name: "Retry" }));

  expect(await screen.findByText("dhcp")).toBeInTheDocument();
  expect(screen.getByText(/no ipxe boot attempt is recorded/i)).toBeInTheDocument();
  expect(getClientBootDiagnostics).toHaveBeenCalledTimes(2);
});
