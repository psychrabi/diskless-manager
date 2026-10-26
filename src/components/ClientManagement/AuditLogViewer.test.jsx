import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import AuditLogViewer from "./AuditLogViewer";

const mocks = vi.hoisted(() => ({
  getAuditLogs: vi.fn(),
  getAuditEvents: vi.fn(),
  error: vi.fn(),
}));

vi.mock("@/api/modules/control", () => ({
  getAuditLogs: mocks.getAuditLogs,
  getAuditEvents: mocks.getAuditEvents,
}));
vi.mock("@/store/useAppStore", () => ({
  useAppStore: (selector) => selector({ clients: [] }),
}));
vi.mock("@/store/useToastStore", () => ({
  useToastStore: () => ({ error: mocks.error }),
}));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.getAuditLogs.mockResolvedValue({ logs: [] });
  mocks.getAuditEvents.mockResolvedValue({ events: [] });
});

it("switches from client control logs to administrative activity", async () => {
  render(<AuditLogViewer isOpen onClose={vi.fn()} />);

  await waitFor(() => expect(mocks.getAuditLogs).toHaveBeenCalledOnce());
  expect(mocks.getAuditEvents).not.toHaveBeenCalled();

  fireEvent.click(
    screen.getByRole("tab", { name: "Administrative Activity" }),
  );

  await waitFor(() => expect(mocks.getAuditEvents).toHaveBeenCalledOnce());
  expect(
    await screen.findByText("No administrative activity found"),
  ).toBeInTheDocument();
});
