import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import SystemAuditLogView from "./SystemAuditLogView";

const mocks = vi.hoisted(() => ({ getAuditEvents: vi.fn() }));

vi.mock("@/api/modules/control", () => ({
  getAuditEvents: mocks.getAuditEvents,
}));

const event = {
  id: "event-1",
  actor: "admin",
  actor_role: "admin",
  ip: "192.0.2.10",
  module: "clients",
  action: "shutdown",
  method: "POST",
  path: "/api/clients/client-1/shutdown",
  status: "failed",
  status_code: 409,
  details: "Conflict",
  timestamp: "2026-10-26T10:00:00Z",
};

beforeEach(() => {
  vi.resetAllMocks();
});

it("lists administrative events and sends the actor filter", async () => {
  mocks.getAuditEvents.mockResolvedValue({ events: [event] });
  render(<SystemAuditLogView />);

  expect((await screen.findAllByText("admin")).length).toBeGreaterThan(0);
  expect(screen.getAllByText("clients").length).toBeGreaterThan(0);
  expect(screen.getAllByText("failed").length).toBeGreaterThan(0);
  expect(screen.getAllByText(/shutdown/).length).toBeGreaterThan(0);
  expect(screen.getAllByText(/192\.0\.2\.10/).length).toBeGreaterThan(0);

  fireEvent.change(screen.getByLabelText("Actor"), {
    target: { value: "operator" },
  });

  await waitFor(() =>
    expect(mocks.getAuditEvents).toHaveBeenCalledWith(
      expect.objectContaining({ actor: "operator" }),
    ),
  );
});

it("offers a retry after a failed load", async () => {
  mocks.getAuditEvents.mockRejectedValueOnce(new Error("network down"));
  render(<SystemAuditLogView />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network down");

  mocks.getAuditEvents.mockResolvedValueOnce({ events: [] });
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  await waitFor(() => expect(mocks.getAuditEvents).toHaveBeenCalledTimes(2));
  expect(
    await screen.findByText("No administrative activity found"),
  ).toBeInTheDocument();
});
