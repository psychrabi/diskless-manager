import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  restartAllServices: vi.fn(),
  fetchServices: vi.fn(),
  error: vi.fn(),
  services: [],
  servicesError: null,
}));

vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => mocks.confirm }));
vi.mock("@/api/modules/services", () => ({ restartAllServices: mocks.restartAllServices }));
vi.mock("@/store/useToastStore", () => ({
  useToastStore: (selector) => selector({ error: mocks.error }),
}));
vi.mock("@/store/useAppStore", () => ({
  useAppStore: (selector) =>
    selector({
      services: mocks.services,
      servicesError: mocks.servicesError,
      fetchServices: mocks.fetchServices,
    }),
}));

import ServicesStatus from "./ServicesStatus";

beforeEach(() => {
  vi.resetAllMocks();
  mocks.services = [];
  mocks.servicesError = null;
});

it("restarts services and refreshes status only after confirmation", async () => {
  mocks.confirm.mockResolvedValue(true);
  render(<ServicesStatus />);

  fireEvent.click(screen.getByTitle("Restart all services"));

  await waitFor(() => expect(mocks.restartAllServices).toHaveBeenCalledOnce());
  expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({
    confirmText: "Restart",
    cancelText: "Cancel",
  }));
  expect(mocks.fetchServices).toHaveBeenCalledOnce();
});

it("does not restart services when confirmation is canceled", async () => {
  mocks.confirm.mockResolvedValue(false);
  render(<ServicesStatus />);

  fireEvent.click(screen.getByTitle("Restart all services"));

  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  expect(mocks.restartAllServices).not.toHaveBeenCalled();
  expect(mocks.fetchServices).not.toHaveBeenCalled();
});

it("reports a restart failure instead of leaving an unhandled rejection", async () => {
  mocks.confirm.mockResolvedValue(true);
  mocks.restartAllServices.mockRejectedValue(new Error("systemd denied"));
  render(<ServicesStatus />);

  fireEvent.click(screen.getByTitle("Restart all services"));

  await waitFor(() => expect(mocks.error).toHaveBeenCalledWith("Services", "systemd denied"));
  expect(mocks.fetchServices).not.toHaveBeenCalled();
});

it("shows a retryable service load error", () => {
  mocks.servicesError = "systemd unavailable";
  render(<ServicesStatus />);

  expect(screen.getByRole("alert")).toHaveTextContent(
    "Failed to load services: systemd unavailable",
  );
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(mocks.fetchServices).toHaveBeenCalledOnce();
});
