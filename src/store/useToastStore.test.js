import { afterEach, expect, it, vi } from "vitest";
import { useToastStore } from "./useToastStore";

afterEach(() => {
  useToastStore.getState().clear();
  vi.unstubAllGlobals();
});

it("creates distinct, dismissible toasts when randomUUID is unavailable on LAN HTTP", () => {
  vi.stubGlobal("crypto", {});
  const first = useToastStore.getState().success("Saved", undefined, 0);
  const second = useToastStore.getState().error("Failed", undefined, 0);

  expect(first).not.toBe(second);
  expect(useToastStore.getState().toasts.map((toast) => toast.title)).toEqual([
    "Saved", "Failed",
  ]);
  useToastStore.getState().dismiss(first);
  expect(useToastStore.getState().toasts.map((toast) => toast.title)).toEqual([
    "Failed",
  ]);
});
