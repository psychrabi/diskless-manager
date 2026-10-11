import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { SnapshotsList } from "./SnapshotsList";

vi.mock("@/hooks/useMasterManager", () => ({
  useMasterManager: () => ({ handleDeleteSnapshot: vi.fn(), handleRollbackSnapshot: vi.fn() }),
}));

it("hides the tree after the last snapshot is removed", () => {
  const { rerender } = render(<SnapshotsList master={{
    name: "cachyos", snapshots: [{ name: "ready", created: "2026-10-08", used: "1M" }],
  }} />);
  expect(screen.getByRole("tree")).toBeInTheDocument();
  expect(screen.getByTestId("snapshot-actions")).toHaveClass("flex-col", "sm:flex-row");
  rerender(<SnapshotsList master={{ name: "cachyos", snapshots: [] }} />);
  expect(screen.queryByRole("tree")).not.toBeInTheDocument();
  expect(screen.queryByText("Restore Points")).not.toBeInTheDocument();
});
