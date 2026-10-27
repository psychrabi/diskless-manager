import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  handleDeleteSnapshot: vi.fn(),
  handleRollbackSnapshot: vi.fn(),
  handleApplySnapshot: vi.fn(),
}));

vi.mock("@/hooks/useMasterManager", () => ({
  useMasterManager: () => mocks,
}));

import { SnapshotsList } from "./SnapshotsList";

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

it("passes the exact newer snapshots to the rollback confirmation", () => {
  mocks.handleRollbackSnapshot.mockClear();
  render(<SnapshotsList master={{
    name: "cachyos",
    snapshots: [
      { name: "snap-b", created: "2026-10-02", used: "1M" },
      { name: "snap-a", created: "2026-10-01", used: "1M" },
      { name: "snap-c", created: "2026-10-03", used: "1M" },
    ],
  }} />);

  fireEvent.click(screen.getByTitle("Rollback snap-a"));

  expect(mocks.handleRollbackSnapshot).toHaveBeenCalledWith(
    "snap-a",
    "cachyos",
    [
      { name: "snap-b", created: "2026-10-02", used: "1M" },
      { name: "snap-c", created: "2026-10-03", used: "1M" },
    ],
  );
});

it("applies a snapshot to a new image without touching the chain", () => {
  mocks.handleApplySnapshot.mockClear();
  render(<SnapshotsList master={{
    name: "cachyos",
    snapshots: [
      { name: "snap-a", created: "2026-10-01", used: "1M" },
      { name: "snap-b", created: "2026-10-02", used: "1M" },
    ],
  }} />);

  fireEvent.click(screen.getByTitle("Apply snap-a as new image"));

  expect(mocks.handleApplySnapshot).toHaveBeenCalledWith("snap-a", "cachyos");
  expect(mocks.handleRollbackSnapshot).not.toHaveBeenCalled();
  expect(mocks.handleDeleteSnapshot).not.toHaveBeenCalled();
});
