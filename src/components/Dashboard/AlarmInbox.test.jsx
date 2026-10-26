import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
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

it("renders active alarms and acknowledges them", async () => {
  mocks.listAlarms.mockResolvedValue(summary);
  mocks.acknowledgeAlarm.mockResolvedValue(undefined);
  render(<AlarmInbox />);

  expect(await screen.findByText("DHCP Server is stopped")).toBeInTheDocument();
  expect(screen.getByText("1 critical, 0 warning")).toBeInTheDocument();

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

  fireEvent.click(screen.getByRole("button", { name: "Acknowledge" }));

  await waitFor(() =>
    expect(mocks.acknowledgeAlarm).toHaveBeenCalledWith("service:dhcp"),
  );
  expect(await screen.findByText(/Acknowledged by admin/)).toBeInTheDocument();
  expect(mocks.listAlarms).toHaveBeenCalledTimes(2);
});

it("shows a retry when loading fails", async () => {
  mocks.listAlarms.mockRejectedValueOnce(new Error("server unavailable"));
  render(<AlarmInbox />);

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
  render(<AlarmInbox />);

  expect(await screen.findByText("No active alarms.")).toBeInTheDocument();
});
