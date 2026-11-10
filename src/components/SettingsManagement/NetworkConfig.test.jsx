import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi, beforeEach } from "vitest";
import { useAppStore } from "@/store/useAppStore";
import NetworkConfig from "./NetworkConfig";

const settings = vi.hoisted(() => ({
  fetchInterfaces: vi.fn().mockResolvedValue(["eth0"]), detectNetwork: vi.fn(),
  applyNetworkSettings: vi.fn(), updateServer: vi.fn().mockResolvedValue(true),
}));
vi.mock("@/hooks/useSettings", () => ({ useSettings: () => settings }));

const confirm = vi.hoisted(() => vi.fn());
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => confirm }));

const serverSettings = {
  interface: ["eth0"], ip_address: "192.168.1.250", netmask: "255.255.255.0",
  gateway: "192.168.1.254", dns: ["9.9.9.9"], hostname: "pxeserver", domain: "local",
};

beforeEach(() => {
  confirm.mockReset();
  settings.applyNetworkSettings.mockClear();
  settings.updateServer.mockClear();
});

it("saves setup network settings without applying a static IP and refreshes readiness", async () => {
  useAppStore.setState({
    appConfig: { settings: { server: {
      interface: ["eth0"], ip_address: "192.168.1.250", netmask: "255.255.255.0",
      gateway: "192.168.1.254", dns: ["9.9.9.9"], hostname: "pxeserver", domain: "local",
    } } },
    fetchServerInfo: vi.fn().mockResolvedValue({}),
  });
  const onSaved = vi.fn();
  render(<NetworkConfig applyOnSave={false} onSaved={onSaved} />);
  await screen.findByText("eth0");
  await userEvent.setup().click(screen.getByRole("button", { name: "Save Configuration" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(settings.updateServer).toHaveBeenCalledWith(expect.objectContaining({ interface: ["eth0"], dns: ["9.9.9.9"] }));
  expect(settings.applyNetworkSettings).not.toHaveBeenCalled();
});

it("stacks the network sections until wide layouts and keeps footer actions on one line", async () => {
  const { container } = render(<NetworkConfig />);
  await screen.findByText("eth0");
  expect(container.querySelector('[data-testid="network-sections"]')).toHaveClass("grid-cols-1", "xl:grid-cols-2");
  expect(container.querySelector('[data-testid="network-actions"]')).toHaveClass("flex-nowrap");
  const detect = screen.getByRole("button", { name: "Auto-Detect" });
  expect(detect.querySelector("svg")).toHaveAttribute("data-icon", "inline-start");
});

it("confirms before applying saved network settings and names the interface and address", async () => {
  confirm.mockResolvedValue(true);
  useAppStore.setState({
    appConfig: { settings: { server: serverSettings } },
    fetchServerInfo: vi.fn().mockResolvedValue({}),
  });
  render(<NetworkConfig />);
  await screen.findByText("eth0");
  await userEvent.setup().click(screen.getByRole("button", { name: "Save & Apply" }));

  await waitFor(() => expect(confirm).toHaveBeenCalled());
  expect(confirm.mock.calls[0][0].description).toContain("eth0");
  expect(confirm.mock.calls[0][0].description).toContain("192.168.1.250");
  expect(confirm.mock.calls[0][0].confirmVariant).toBe("destructive");
  await waitFor(() => expect(settings.applyNetworkSettings).toHaveBeenCalled());
});

it("keeps the saved configuration without applying it when the confirmation is declined", async () => {
  confirm.mockResolvedValue(false);
  useAppStore.setState({
    appConfig: { settings: { server: serverSettings } },
    fetchServerInfo: vi.fn().mockResolvedValue({}),
  });
  render(<NetworkConfig />);
  await screen.findByText("eth0");
  await userEvent.setup().click(screen.getByRole("button", { name: "Save & Apply" }));

  await waitFor(() => expect(confirm).toHaveBeenCalled());
  expect(settings.updateServer).toHaveBeenCalled();
  expect(settings.applyNetworkSettings).not.toHaveBeenCalled();
});
