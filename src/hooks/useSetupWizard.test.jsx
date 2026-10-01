import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useSetupWizard } from "./useSetupWizard";

const api = vi.hoisted(() => ({
  getSetupStatus: vi.fn(), completeSetup: vi.fn(), checkPrivilegedAccess: vi.fn(),
  listDisks: vi.fn(), createZfsPool: vi.fn(), installService: vi.fn(),
  saveServiceConfig: vi.fn(), getServiceConfig: vi.fn(),
  getSettings: vi.fn(), saveSettings: vi.fn(), configureService: vi.fn(), restartService: vi.fn(),
  readConfig: vi.fn(), checkDependencies: vi.fn(), listServices: vi.fn(),
}));
vi.mock("@/api/modules/system", () => api);
vi.mock("@/api/modules/disks", () => api);
vi.mock("@/api/modules/services", () => api);
vi.mock("@/api/modules/config", () => api);

beforeEach(() => {
  vi.resetAllMocks();
  api.getSetupStatus.mockResolvedValue({ completed: false, ready: false, missing: ["samba", "boot"] });
  api.checkPrivilegedAccess.mockResolvedValue({ authorized: true });
  api.listDisks.mockResolvedValue(["sda"]);
  api.getServiceConfig.mockResolvedValue({ text: "" });
  api.getSettings.mockResolvedValue({ samba: { enabled: true } });
  api.readConfig.mockResolvedValue({ settings: { zpool_name: "diskless", samba: { enabled: true } } });
  api.checkDependencies.mockResolvedValue([]);
  api.listServices.mockResolvedValue([]);
});

it("does not advance the boot step when writing the script fails", async () => {
  api.getSetupStatus.mockResolvedValue({ completed: false, ready: false, missing: ["boot"] });
  api.saveServiceConfig.mockRejectedValue(new Error("write denied"));
  const { result } = renderHook(() => useSetupWizard());
  await waitFor(() => expect(result.current.activeStep).toBe(8));
  await act(() => result.current.handleBootScriptSubmit("#!ipxe\nshell"));
  expect(result.current.activeStep).toBe(8);
});

it("uses authoritative setup authorization and skips privileged inventory until authorized", async () => {
  api.getSetupStatus.mockResolvedValue({ completed: false, ready: false, missing: ["authorization", "dependencies", "storage", "dhcp", "settings", "tftp", "http", "samba", "boot"] });
  api.listDisks.mockRejectedValue(new Error("Privileged inventory unavailable"));
  const { result } = renderHook(() => useSetupWizard());
  await waitFor(() => expect(result.current.checking).toBe(false));
  await waitFor(() => expect(result.current.setupStatus).not.toBeNull());
  expect(result.current.activeStep).toBe(1);
  expect(result.current.privilegedAccessGranted).toBe(false);
  expect(api.listDisks).not.toHaveBeenCalled();
});

it("keeps completion failure visible after refreshing readiness", async () => {
  api.getSetupStatus.mockResolvedValue({ completed: false, ready: true, missing: [] });
  api.completeSetup.mockRejectedValue(new Error("Cannot save completion marker"));
  const { result } = renderHook(() => useSetupWizard());
  await waitFor(() => expect(result.current.activeStep).toBe(9));
  await act(async () => expect(await result.current.finishSetup()).toBe(false));
  expect(result.current.setupError).toBe("Cannot save completion marker");
});

it("persists Samba settings before configuring and restarting the service", async () => {
  const { result } = renderHook(() => useSetupWizard());
  await waitFor(() => expect(result.current.activeStep).toBe(7));
  const config = { enabled: true, share_name: "games", share_path: "/srv/games" };
  await act(() => result.current.handleSambaSubmit(config));
  expect(api.saveSettings).toHaveBeenCalledWith({ samba: config });
  expect(api.configureService).toHaveBeenCalledWith("samba");
  expect(api.restartService).toHaveBeenCalledWith("samba");
  expect(result.current.activeStep).toBe(7); // Readiness still reports Samba missing.
});
