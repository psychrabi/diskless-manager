import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import PendingRecovery from ".";
import { getPendingClientRecoveries } from "@/api/modules/reconciliation";

vi.mock("@/api/modules/reconciliation", () => ({
  getPendingClientRecoveries: vi.fn(),
  retryPendingClientRecovery: vi.fn(),
}));

beforeEach(() => {
  getPendingClientRecoveries.mockResolvedValue({ recoveries: [{
    client_id: "client-1",
    client_name: "client-one",
    kind: "boot_failure",
    failures: 2,
    retry_after: 0,
    last_error: "target unavailable",
  }] });
});

it("shows boot failures and omits manual retry", async () => {
  render(<PendingRecovery />);
  expect(await screen.findByText("Boot provisioning" )).toBeInTheDocument();
  expect(screen.getByText("target unavailable")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Retry now" })).not.toBeInTheDocument();
});
