import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useMasterManager } from "./useMasterManager";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(), success: vi.fn(), error: vi.fn(), fetchMasters: vi.fn(),
  deleteSnapshot: vi.fn(), rollbackImageSnapshot: vi.fn(), setDefaultImage: vi.fn(), deleteImage: vi.fn(),
}));
vi.mock("@/api/modules/images", () => ({
  deleteSnapshot: mocks.deleteSnapshot, rollbackImageSnapshot: mocks.rollbackImageSnapshot,
  setDefaultImage: mocks.setDefaultImage, deleteImage: mocks.deleteImage,
}));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => mocks.confirm }));
vi.mock("@/store/useAppStore", () => ({ useAppStore: (selector) => selector({ fetchMasters: mocks.fetchMasters }) }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ success: mocks.success, error: mocks.error }) }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.confirm.mockResolvedValue(true);
  mocks.deleteSnapshot.mockResolvedValue({});
  mocks.fetchMasters.mockResolvedValue(undefined);
});

it("shows success after deleting snapshot even when API has no message", async () => {
  const { result } = renderHook(() => useMasterManager());
  await act(() => result.current.handleDeleteSnapshot("ready", "diskless/windows"));
  await act(async () => {});
  expect(mocks.deleteSnapshot).toHaveBeenCalledWith("diskless/windows", "ready");
  expect(mocks.fetchMasters).toHaveBeenCalledOnce();
  expect(mocks.success).toHaveBeenCalledWith("Image Management", "Snapshot deleted successfully");
});

it("shows an error when snapshot deletion fails", async () => {
  mocks.deleteSnapshot.mockRejectedValue(new Error("delete denied"));
  const { result } = renderHook(() => useMasterManager());
  await act(() => result.current.handleDeleteSnapshot("ready", "diskless/windows"));
  await act(async () => {});
  expect(mocks.error).toHaveBeenCalledWith("delete denied");
});

it("lists the exact newer snapshots before approving pruning", async () => {
  mocks.rollbackImageSnapshot.mockResolvedValue({ message: "restored" });
  const { result } = renderHook(() => useMasterManager());
  const newer = [{ name: "snap-b" }, { name: "snap-c" }];

  await act(() =>
    result.current.handleRollbackSnapshot("stable", "pool/windows", newer)
  );

  expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({
    description: expect.stringContaining("snap-b, snap-c"),
    confirmVariant: "destructive",
  }));
  expect(mocks.rollbackImageSnapshot).toHaveBeenCalledWith("pool/windows", "stable", ["snap-b", "snap-c"]);
  expect(mocks.fetchMasters).toHaveBeenCalledOnce();
});

it("does not request pruning when the selected snapshot is already latest", async () => {
  mocks.rollbackImageSnapshot.mockResolvedValue({ message: "restored" });
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleRollbackSnapshot("stable", "pool/windows", []));

  expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({
    description: expect.stringContaining("No newer recovery points will be deleted"),
    confirmVariant: "primary",
  }));
  expect(mocks.rollbackImageSnapshot).toHaveBeenCalledWith("pool/windows", "stable", []);
});

it("does not rollback when snapshot confirmation is canceled", async () => {
  mocks.confirm.mockResolvedValue(false);
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleRollbackSnapshot("stable", "pool/windows"));

  expect(mocks.rollbackImageSnapshot).not.toHaveBeenCalled();
});
