import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useMasterManager } from "./useMasterManager";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(), success: vi.fn(), error: vi.fn(), fetchMasters: vi.fn(),
  deleteSnapshot: vi.fn(), rollbackImageSnapshot: vi.fn(), applySnapshot: vi.fn(),
  setDefaultImage: vi.fn(), deleteImage: vi.fn(),
}));
vi.mock("@/api/modules/images", () => ({
  deleteSnapshot: mocks.deleteSnapshot, rollbackImageSnapshot: mocks.rollbackImageSnapshot,
  applySnapshot: mocks.applySnapshot,
  setDefaultImage: mocks.setDefaultImage, deleteImage: mocks.deleteImage,
}));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => mocks.confirm }));
vi.mock("@/store/useAppStore", () => ({ useAppStore: (selector) => selector({ fetchMasters: mocks.fetchMasters }) }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ success: mocks.success, error: mocks.error }) }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.confirm.mockResolvedValue(true);
  mocks.deleteSnapshot.mockResolvedValue({});
  mocks.applySnapshot.mockResolvedValue({ name: "pool/windows-restored" });
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

it("applies a restore point to a new image and refreshes the list", async () => {
  mocks.confirm.mockResolvedValue("pool/windows-restored");
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleApplySnapshot("stable", "pool/windows"));
  await act(async () => {});

  expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({
    showInput: true,
    defaultValue: expect.stringMatching(/^windows-stable-restore-\d{8}-\d{6}$/),
  }));
  expect(mocks.applySnapshot).toHaveBeenCalledWith(
    "pool/windows",
    "stable",
    "pool/windows-restored"
  );
  expect(mocks.fetchMasters).toHaveBeenCalledOnce();
  expect(mocks.success).toHaveBeenCalledWith(
    "Image Management",
    'Created pool/windows-restored from restore point "stable"'
  );
});

it("does not apply when restore-point confirmation is canceled", async () => {
  mocks.confirm.mockResolvedValue(false);
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleApplySnapshot("stable", "pool/windows"));

  expect(mocks.applySnapshot).not.toHaveBeenCalled();
  expect(mocks.fetchMasters).not.toHaveBeenCalled();
});

it("rejects a blank new image name", async () => {
  mocks.confirm.mockResolvedValue("   ");
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleApplySnapshot("stable", "pool/windows"));

  expect(mocks.applySnapshot).not.toHaveBeenCalled();
  expect(mocks.error).toHaveBeenCalledWith(
    "Image Management",
    "Enter a name for the new image"
  );
});

it("shows an error when applying a restore point fails", async () => {
  mocks.confirm.mockResolvedValue("pool/windows-restored");
  mocks.applySnapshot.mockRejectedValue({ error: "image already exists" });
  const { result } = renderHook(() => useMasterManager());

  await act(() => result.current.handleApplySnapshot("stable", "pool/windows"));

  expect(mocks.error).toHaveBeenCalledWith("image already exists");
  expect(mocks.fetchMasters).not.toHaveBeenCalled();
});
