import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useClientActions } from "/home/rabistha/Documents/Projects/diskless-manager/src/hooks/useClientActions.js";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn(),
  updateClient: vi.fn(), deleteClient: vi.fn(), createSnapshot: vi.fn(),
  requestInventoryBoot: vi.fn(),
}));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => mocks.confirm }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ success: mocks.success, error: mocks.error, info: mocks.info }) }));
vi.mock("../api/modules/clients", () => ({ updateClient: mocks.updateClient, deleteClient: mocks.deleteClient }));
vi.mock("../api/modules/images", () => ({ createSnapshot: mocks.createSnapshot }));
vi.mock("../api/modules/networkDrivers", () => ({ requestInventoryBoot: mocks.requestInventoryBoot }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.confirm.mockResolvedValue("save-now");
  mocks.createSnapshot.mockResolvedValue({ message: "Snapshot created" });
  mocks.updateClient.mockResolvedValue({});
});

it("snapshots Super changes then switches the client back to its clone", async () => {
  const fetchData = vi.fn();
  const closeContextMenu = vi.fn();
  const { result } = renderHook(() => useClientActions(fetchData, closeContextMenu));

  await act(() => result.current.saveSuper({
    id: "client-1", name: "PC001", master: "diskless/image-disk/windows11",
    mode: "super", status: "Offline",
  }));

  expect(mocks.createSnapshot).toHaveBeenCalledWith("diskless/image-disk/windows11", "save-now");
  expect(mocks.updateClient).toHaveBeenCalledWith("client-1", { action: "super", make_super: false });
  expect(fetchData).toHaveBeenCalledOnce();
  expect(closeContextMenu).toHaveBeenCalledOnce();
});

it("leaves Super enabled when saving the snapshot fails", async () => {
  mocks.createSnapshot.mockRejectedValue(new Error("snapshot failed"));
  const { result } = renderHook(() => useClientActions());

  await act(() => result.current.saveSuper({
    id: "client-1", name: "PC001", master: "diskless/image-disk/windows11",
    mode: "super", status: "Offline",
  }));

  expect(mocks.updateClient).not.toHaveBeenCalled();
  expect(mocks.error).toHaveBeenCalledWith("Client Management", "Failed to save Super changes or return to writeback mode: snapshot failed");
});

it("requests an inventory boot after confirmation", async () => {
  mocks.confirm.mockResolvedValue(true);
  mocks.requestInventoryBoot.mockResolvedValue({ message: "Inventory boot requested for 'PC009'" });
  const fetchData = vi.fn();
  const closeContextMenu = vi.fn();
  const { result } = renderHook(() => useClientActions(fetchData, closeContextMenu));

  await act(() => result.current.inventoryBoot({ id: "client-9", name: "PC009" }));

  expect(mocks.requestInventoryBoot).toHaveBeenCalledWith("client-9");
  expect(mocks.success).toHaveBeenCalledWith("Client Management", "Inventory boot requested for 'PC009'");
  expect(fetchData).toHaveBeenCalledOnce();
  expect(closeContextMenu).toHaveBeenCalledOnce();
});

it("surfaces inventory boot failures without refreshing", async () => {
  mocks.confirm.mockResolvedValue(true);
  mocks.requestInventoryBoot.mockRejectedValue(new Error("already provisioned"));
  const fetchData = vi.fn();
  const { result } = renderHook(() => useClientActions(fetchData, vi.fn()));

  await act(() => result.current.inventoryBoot({ id: "client-9", name: "PC009" }));

  expect(mocks.error).toHaveBeenCalledWith(
    "Client Management",
    "Failed to execute inventoryBoot: already provisioned"
  );
  expect(fetchData).not.toHaveBeenCalled();
});
