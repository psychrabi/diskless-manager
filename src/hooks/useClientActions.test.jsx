import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useClientActions } from "/home/rabistha/Documents/Projects/diskless-manager/src/hooks/useClientActions.js";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn(),
  updateClient: vi.fn(), deleteClient: vi.fn(), createSnapshot: vi.fn(),
  requestInventoryBoot: vi.fn(),
  enterSuper: vi.fn(), saveSuper: vi.fn(), discardSuper: vi.fn(),
}));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => mocks.confirm }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ success: mocks.success, error: mocks.error, info: mocks.info }) }));
vi.mock("../api/modules/clients", () => ({ updateClient: mocks.updateClient, deleteClient: mocks.deleteClient, enterSuper: mocks.enterSuper, saveSuper: mocks.saveSuper, discardSuper: mocks.discardSuper }));
vi.mock("../api/modules/images", () => ({ createSnapshot: mocks.createSnapshot }));
vi.mock("../api/modules/networkDrivers", () => ({ requestInventoryBoot: mocks.requestInventoryBoot }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.confirm.mockResolvedValue("save-now");
  mocks.createSnapshot.mockResolvedValue({ message: "Snapshot created" });
  mocks.updateClient.mockResolvedValue({});
});

it("saves an active managed Super session through the lifecycle API", async () => {
  const fetchData = vi.fn();
  const closeContextMenu = vi.fn();
  const { result } = renderHook(() => useClientActions(fetchData, closeContextMenu));

  await act(() => result.current.saveSuper(
    { id: "client-1", name: "PC001", status: "Offline" },
    { session: { state: "active", verification_only: false } },
  ));

  expect(mocks.saveSuper).toHaveBeenCalledWith("client-1");
  expect(fetchData).toHaveBeenCalledOnce();
  expect(closeContextMenu).toHaveBeenCalledOnce();
});

it("confirms discard and calls the lifecycle API", async () => {
  const { result } = renderHook(() => useClientActions());

  await act(() => result.current.discardSuper(
    { id: "client-1", name: "PC001", status: "Offline" },
    { session: { state: "active", verification_only: false } },
  ));

  expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({
    title: "Discard Super Changes",
    description: expect.stringContaining("restore its prior image assignment"),
    confirmText: "Discard Changes",
  }));
  expect(mocks.discardSuper).toHaveBeenCalledWith("client-1");
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

it("starts a confirmed disposable verification session through the Super API", async () => {
  mocks.confirm.mockResolvedValue(true);
  const fetchData = vi.fn();
  const closeContextMenu = vi.fn();
  const { result } = renderHook(() => useClientActions(fetchData, closeContextMenu));

  await act(() => result.current.startSuper({ id: "client-1", name: "PC001", status: "Offline" }, true));

  expect(mocks.enterSuper).toHaveBeenCalledWith("client-1", { verification_only: true, confirmed: true });
  expect(fetchData).toHaveBeenCalledOnce();
  expect(closeContextMenu).toHaveBeenCalledOnce();
});

it("does not save a verification-only session", async () => {
  const { result } = renderHook(() => useClientActions());

  await act(() => result.current.saveSuper({ id: "client-1", name: "PC001", status: "Offline" }, { session: { verification_only: true, state: "active" } }));

  expect(mocks.saveSuper).not.toHaveBeenCalled();
  expect(mocks.error).toHaveBeenCalledWith("Client Management", "Verification-only sessions cannot be saved.");
});

it("blocks starting or finishing Super sessions while the client is online or recovering", async () => {
  const { result } = renderHook(() => useClientActions());

  await act(() => result.current.startSuper({ id: "client-1", name: "PC001", status: "Online" }));
  await act(() => result.current.startSuper({ id: "client-1", name: "PC001", status: "Offline" }, false, { storage_recovery: true }));
  await act(() => result.current.discardSuper(
    { id: "client-1", name: "PC001", status: "Offline" },
    { session: { state: "active" }, storage_recovery: true },
  ));

  expect(mocks.enterSuper).not.toHaveBeenCalled();
  expect(mocks.discardSuper).not.toHaveBeenCalled();
  expect(mocks.error).toHaveBeenCalledTimes(3);
});

it("blocks Super actions when the API reports lowercase online status", async () => {
  const client = { id: "client-1", name: "PC001", status: "online" };
  const activeSession = { session: { state: "active", verification_only: false } };
  const { result } = renderHook(() => useClientActions());

  await act(() => result.current.startSuper(client));
  await act(() => result.current.saveSuper(client, activeSession));
  await act(() => result.current.discardSuper(client, activeSession));

  expect(mocks.enterSuper).not.toHaveBeenCalled();
  expect(mocks.saveSuper).not.toHaveBeenCalled();
  expect(mocks.discardSuper).not.toHaveBeenCalled();
  expect(mocks.confirm).not.toHaveBeenCalled();
  expect(mocks.error).toHaveBeenCalledTimes(3);
});
