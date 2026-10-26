import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ImageHealthDialog from "./ImageHealthDialog";

const mocks = vi.hoisted(() => ({ getImageHealth: vi.fn() }));

vi.mock("@/api/modules/images", () => ({
  getImageHealth: mocks.getImageHealth,
}));

const health = {
  image_id: "master-1",
  status: "pass",
  checks: [
    {
      id: "storage",
      label: "Storage volume",
      status: "pass",
      detail: "pool/image is present",
    },
    {
      id: "windows-filesystem",
      label: "Windows filesystem",
      status: "pass",
      detail: "NTFS or BitLocker volume found",
    },
  ],
  boot: {
    partition_table: "gpt",
    filesystem: null,
    partitions: [
      {
        name: "zd0p1",
        size: 536870912,
        filesystem: "vfat",
        part_type: "EFI System",
        part_flags: "boot",
      },
    ],
  },
};

beforeEach(() => {
  vi.resetAllMocks();
});

it("shows health checks and partition metadata", async () => {
  mocks.getImageHealth.mockResolvedValue(health);
  render(
    <ImageHealthDialog
      open
      onOpenChange={vi.fn()}
      image={{ id: "master-1", name: "pool/image" }}
    />,
  );

  expect(await screen.findByText("Storage volume")).toBeInTheDocument();
  expect(screen.getByText("Windows filesystem")).toBeInTheDocument();
  expect(screen.getByText("gpt")).toBeInTheDocument();
  expect(screen.getByText("zd0p1")).toBeInTheDocument();
  expect(screen.getByText("EFI System")).toBeInTheDocument();
  expect(mocks.getImageHealth).toHaveBeenCalledWith("master-1");
});

it("offers a retry after a failed inspection", async () => {
  mocks.getImageHealth.mockRejectedValueOnce(new Error("probe unavailable"));
  render(
    <ImageHealthDialog
      open
      onOpenChange={vi.fn()}
      image={{ id: "master-1", name: "pool/image" }}
    />,
  );

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "probe unavailable",
  );

  mocks.getImageHealth.mockResolvedValueOnce(health);
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  await waitFor(() => expect(mocks.getImageHealth).toHaveBeenCalledTimes(2));
  expect(await screen.findByText("Storage volume")).toBeInTheDocument();
});
