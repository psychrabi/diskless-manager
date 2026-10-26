import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import LegacySuperExitDialog from "./LegacySuperExitDialog";
import { useAuth } from "@/contexts/auth";
import { useConfirm } from "@/contexts/confirmDialog";
import { exitLegacySuper } from "@/api/modules/clients";

vi.mock("@/contexts/auth", () => ({ useAuth: vi.fn() }));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: vi.fn() }));
vi.mock("@/api/modules/clients", () => ({ exitLegacySuper: vi.fn() }));

const props = {
  client: { id: "c1", name: "PC001", master: "pool/windows", status: "Offline" },
  masters: [
    { name: "pool/windows", snapshots: [{ name: "pool/windows@ready" }, { name: "old" }] },
    { name: "pool/linux", snapshots: [{ name: "pool/linux@other" }] },
  ],
  isOpen: true,
  onClose: vi.fn(),
  refresh: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  useAuth.mockReturnValue({ user: { role: "admin" } });
  useConfirm.mockReturnValue(vi.fn().mockResolvedValue(true));
  exitLegacySuper.mockResolvedValue({});
});

it("offers only assigned-master snapshots and confirms before exiting", async () => {
  render(<LegacySuperExitDialog {...props} />);

  const snapshotSelect = screen.getByLabelText("Master snapshot");
  expect([...snapshotSelect.options].map((option) => option.value)).toEqual([
    "",
    "pool/windows@ready",
    "pool/windows@old",
  ]);

  fireEvent.change(snapshotSelect, { target: { value: "pool/windows@ready" } });
  fireEvent.click(screen.getByRole("button", { name: "Exit Legacy Super" }));

  await waitFor(() => expect(exitLegacySuper).toHaveBeenCalledWith("c1", "pool/windows@ready"));
  expect(useConfirm.mock.results[0].value).toHaveBeenCalledWith(expect.objectContaining({
    description: expect.stringContaining("client is offline"),
  }));
  expect(useConfirm.mock.results[0].value).toHaveBeenCalledWith(expect.objectContaining({
    description: expect.stringContaining("direct-master legacy mapping"),
  }));
  expect(useConfirm.mock.results[0].value.mock.calls[0][0].description).toContain("will be replaced");
  expect(props.refresh).toHaveBeenCalled();
});

it("does not show legacy exit controls to non-admin users", () => {
  useAuth.mockReturnValue({ user: { role: "user" } });
  render(<LegacySuperExitDialog {...props} />);
  expect(screen.queryByRole("button", { name: "Exit Legacy Super" })).not.toBeInTheDocument();
});

it("locks recovery retries to the journal snapshot", async () => {
  const client = {
    ...props.client,
    legacy_super_state: "recovery_required",
    legacy_super_snapshot: "pool/windows@ready",
  };
  render(<LegacySuperExitDialog {...props} client={client} />);

  const snapshotSelect = screen.getByLabelText("Master snapshot");
  expect(snapshotSelect).toHaveValue("pool/windows@ready");
  expect(snapshotSelect).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Exit Legacy Super" }));

  await waitFor(() => expect(exitLegacySuper).toHaveBeenCalledWith("c1", "pool/windows@ready"));
});

it("refreshes client state after a failed exit", async () => {
  exitLegacySuper.mockRejectedValue(new Error("storage cleanup incomplete"));
  render(<LegacySuperExitDialog {...props} />);
  fireEvent.change(screen.getByLabelText("Master snapshot"), { target: { value: "pool/windows@ready" } });
  fireEvent.click(screen.getByRole("button", { name: "Exit Legacy Super" }));

  await waitFor(() => expect(props.refresh).toHaveBeenCalled());
});
