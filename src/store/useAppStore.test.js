import { afterEach, beforeEach, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  listClients: vi.fn(),
  listMasters: vi.fn(),
  listImages: vi.fn(),
  listServices: vi.fn(),
}));

vi.mock("../api/modules/clients", () => ({ listClients: api.listClients }));
vi.mock("../api/modules/images", () => ({
  listMasters: api.listMasters,
  listImages: api.listImages,
}));
vi.mock("../api/modules/services", () => ({
  listServices: api.listServices,
  startService: vi.fn(),
  stopService: vi.fn(),
  restartService: vi.fn(),
  enableServiceBoot: vi.fn(),
  disableServiceBoot: vi.fn(),
}));

import { useAppStore } from "./useAppStore";

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  useAppStore.getState().stopClientStatusPolling();
  useAppStore.setState({
    clients: [],
    clientsError: null,
    masters: [],
    mastersError: null,
    services: [],
    servicesError: null,
  });
});

afterEach(() => {
  useAppStore.getState().stopClientStatusPolling();
  vi.useRealTimers();
});

it("refreshes client status on its single polling interval", async () => {
  useAppStore.setState({ clients: [{ id: "client-1", status: "Offline" }] });
  api.listClients.mockResolvedValue([{ id: "client-1", status: "Online" }]);

  useAppStore.getState().startClientStatusPolling();
  useAppStore.getState().startClientStatusPolling();
  await vi.advanceTimersByTimeAsync(30_000);

  expect(api.listClients).toHaveBeenCalledOnce();
  expect(useAppStore.getState().clients[0].status).toBe("Online");
});

it("keeps cached images on fetch failure and clears the error after a successful retry", async () => {
  const cachedMaster = { id: "master-1", name: "cached" };
  useAppStore.setState({ masters: [cachedMaster], mastersError: null });
  api.listMasters.mockRejectedValueOnce(new Error("Catalog unavailable"));
  const log = vi.spyOn(console, "error").mockImplementation(() => {});

  await useAppStore.getState().fetchMasters();
  expect(useAppStore.getState().masters).toEqual([cachedMaster]);
  expect(useAppStore.getState().mastersError).toBe("Catalog unavailable");

  api.listMasters.mockResolvedValueOnce([]);
  await useAppStore.getState().fetchMasters();
  expect(useAppStore.getState().masters).toEqual([]);
  expect(useAppStore.getState().mastersError).toBeNull();
  log.mockRestore();
});

it("keeps cached clients and exposes a retryable load error", async () => {
  const cachedClient = { id: "client-1", name: "PC-001" };
  useAppStore.setState({ clients: [cachedClient], clientsError: null });
  api.listClients.mockRejectedValueOnce(new Error("Network down"));
  const log = vi.spyOn(console, "error").mockImplementation(() => {});

  await useAppStore.getState().fetchClients();
  expect(useAppStore.getState().clients).toEqual([cachedClient]);
  expect(useAppStore.getState().clientsError).toBe("Network down");

  api.listClients.mockResolvedValueOnce([]);
  await useAppStore.getState().fetchClients();
  expect(useAppStore.getState().clients).toEqual([]);
  expect(useAppStore.getState().clientsError).toBeNull();
  log.mockRestore();
});

it("keeps cached services and exposes a retryable load error", async () => {
  const cachedService = { name: "dhcp", running: true };
  useAppStore.setState({ services: [cachedService], servicesError: null });
  api.listServices.mockRejectedValueOnce(new Error("systemd unavailable"));
  const log = vi.spyOn(console, "error").mockImplementation(() => {});

  await useAppStore.getState().fetchServices();
  expect(useAppStore.getState().services).toEqual([cachedService]);
  expect(useAppStore.getState().servicesError).toBe("systemd unavailable");

  api.listServices.mockResolvedValueOnce({ dhcp: cachedService });
  await useAppStore.getState().fetchServices();
  expect(useAppStore.getState().services).toEqual([cachedService]);
  expect(useAppStore.getState().servicesError).toBeNull();
  log.mockRestore();
});
