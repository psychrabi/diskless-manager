import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useAppStore } from "@/store/useAppStore";
import NetworkConfig from "./NetworkConfig";

const settings = vi.hoisted(() => ({
  fetchInterfaces: vi.fn().mockResolvedValue(["eth0"]), detectNetwork: vi.fn(),
  applyNetworkSettings: vi.fn(), updateServer: vi.fn().mockResolvedValue(true),
}));
vi.mock("@/hooks/useSettings", () => ({ useSettings: () => settings }));

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
