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
