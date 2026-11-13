import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import AlarmInbox from "./AlarmInbox";

const mocks = vi.hoisted(() => ({
  listAlarms: vi.fn(),
  acknowledgeAlarm: vi.fn(),
}));

vi.mock("@/api/modules/alarms", () => ({
  listAlarms: mocks.listAlarms,
  acknowledgeAlarm: mocks.acknowledgeAlarm,
}));

const summary = {
  critical: 1,
  warning: 0,
  acknowledged: 0,
  alarms: [
    {
      id: "service:dhcp",
      kind: "service",
      severity: "critical",
      title: "DHCP Server is stopped",
      detail: "Clients cannot complete network boot while this service is stopped.",
      source: "dhcp",
      client_id: null,
      observed_at: null,
      acknowledged_at: null,
      acknowledged_by: null,
    },
  ],
};

beforeEach(() => {
  vi.resetAllMocks();
});

const renderInbox = () =>
  render(
    <MemoryRouter>
      <AlarmInbox />
    </MemoryRouter>,
  );

it("renders active alarms and dismisses them", async () => {
  mocks.listAlarms.mockResolvedValue(summary);
  mocks.acknowledgeAlarm.mockResolvedValue(undefined);
  renderInbox();

  expect(await screen.findByText("DHCP Server is stopped")).toBeInTheDocument();
  expect(screen.getByText("1 critical")).toBeInTheDocument();
  expect(screen.getByText("0 warning")).toBeInTheDocument();

  mocks.listAlarms.mockResolvedValue({
    ...summary,
    critical: 0,
    acknowledged: 1,
    alarms: [
      {
        ...summary.alarms[0],
        acknowledged_at: "2026-10-27T10:00:00Z",
        acknowledged_by: "admin",
      },
    ],
  });

  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));

  await waitFor(() =>
    expect(mocks.acknowledgeAlarm).toHaveBeenCalledWith("service:dhcp"),
  );
  expect(await screen.findByText(/Dismissed by admin/)).toBeInTheDocument();
  expect(mocks.listAlarms).toHaveBeenCalledTimes(2);
});

it("links an alarm to the client it names and to pending recovery", async () => {
  mocks.listAlarms.mockResolvedValue({
    critical: 1,
    warning: 0,
    acknowledged: 0,
    alarms: [
      {
        id: "storage:client-7",
        kind: "storage_recovery",
        severity: "critical",
        title: "Storage recovery pending for Lab 03",
        detail: "1 failed attempt(s); retry after 2026-10-27T10:00:00Z",
        source: "Lab 03",
        client_id: "client-7",
        observed_at: null,
        acknowledged_at: null,
        acknowledged_by: null,
      },
    ],
  });
  renderInbox();

  expect(
    await screen.findByRole("link", { name: /Open client/i }),
  ).toHaveAttribute("href", "/clients?client=client-7");
  expect(screen.getByRole("link", { name: /View recovery/i })).toHaveAttribute(
    "href",
    "/pending-recovery",
  );
});

it("shows a retry when loading fails", async () => {
  mocks.listAlarms.mockRejectedValueOnce(new Error("server unavailable"));
  renderInbox();

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "server unavailable",
  );
  expect(mocks.listAlarms).toHaveBeenCalledTimes(1);

  mocks.listAlarms.mockResolvedValueOnce({
    critical: 0,
    warning: 0,
    acknowledged: 0,
    alarms: [],
  });
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  expect(await screen.findByText("No active alarms.")).toBeInTheDocument();
  expect(mocks.listAlarms).toHaveBeenCalledTimes(2);
});

it("shows an empty state when nothing is wrong", async () => {
  mocks.listAlarms.mockResolvedValue({
    critical: 0,
    warning: 0,
    acknowledged: 0,
    alarms: [],
  });
  renderInbox();

  expect(await screen.findByText("No active alarms.")).toBeInTheDocument();
});
