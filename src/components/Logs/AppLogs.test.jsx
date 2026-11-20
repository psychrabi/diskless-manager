import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { getLogs, clearLogs } from "@/api/modules/logs";
import AppLogs from "./AppLogs";

const confirm = vi.hoisted(() => vi.fn());
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => confirm }));
vi.mock("@/api/modules/logs", () => ({ getLogs: vi.fn(), clearLogs: vi.fn().mockResolvedValue({}) }));

it("preserves captured logs when clearing is declined and clears only after confirmation", async () => {
  const user = userEvent.setup();
  getLogs.mockResolvedValue({ text: "Synthetic diagnostic entry" });
  render(<AppLogs />);
  await screen.findByText("Synthetic diagnostic entry");
  confirm.mockResolvedValue(false);
  await user.click(screen.getByRole("button", { name: "Clear logs" }));
  expect(clearLogs).not.toHaveBeenCalled();
  confirm.mockResolvedValue(true);
  getLogs.mockResolvedValue({ text: "" });
  await user.click(screen.getByRole("button", { name: "Clear logs" }));
  await waitFor(() => expect(clearLogs).toHaveBeenCalledOnce());
  expect(await screen.findByText("No log entries yet.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Clear logs" })).toBeDisabled();
});
