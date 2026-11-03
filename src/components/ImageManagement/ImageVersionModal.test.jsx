import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ImageVersionModal from "./ImageVersionModal";

const mocks = vi.hoisted(() => ({ listImageVersions: vi.fn() }));

vi.mock("@/api/modules/images", () => ({
  listImageVersions: mocks.listImageVersions,
}));

it("lists the versions published for the selected image", async () => {
  mocks.listImageVersions.mockResolvedValue([
    { id: "v2", name: "diskless/image-disk/windows11/2", version: "2" },
  ]);

  render(
    <ImageVersionModal
      openVersionModal
      setOpenVersionModal={vi.fn()}
      selectedImage="diskless/image-disk/windows11"
    />
  );

  expect(
    screen.getByRole("heading", { name: /image versions/i })
  ).toBeInTheDocument();
  expect(await screen.findByTestId("image-versions")).toHaveTextContent("2");
  expect(mocks.listImageVersions).toHaveBeenCalledWith("diskless/image-disk/windows11");
});

it("shows provenance and the pin/lock state of a published version", async () => {
  mocks.listImageVersions.mockResolvedValue([
    {
      id: "v3",
      name: "diskless/image-disk/windows11/2026-11-03",
      version: "2026-11-03",
      source_description: "Committed Windows servicing result",
      published_by: "admin",
      published_at: "2026-11-03T09:00:00Z",
      is_pinned: true,
      is_locked: true,
    },
  ]);

  render(
    <ImageVersionModal
      openVersionModal
      setOpenVersionModal={vi.fn()}
      selectedImage="diskless/image-disk/windows11"
    />
  );

  const list = await screen.findByTestId("image-versions");
  expect(list).toHaveTextContent("Committed Windows servicing result");
  expect(list).toHaveTextContent("Published by admin");
  expect(list).toHaveTextContent("Pinned");
  expect(list).toHaveTextContent("Locked");
});

it("shows an empty state when no version exists", async () => {
  mocks.listImageVersions.mockResolvedValue([]);

  render(
    <ImageVersionModal
      openVersionModal
      setOpenVersionModal={vi.fn()}
      selectedImage="diskless/image-disk/windows11"
    />
  );

  expect(
    await screen.findByText(/no image versions are available yet/i)
  ).toBeInTheDocument();
});

it("closes when openVersionModal switches off", () => {
  const setOpenVersionModal = vi.fn();
  const { rerender } = render(
    <ImageVersionModal
      openVersionModal
      setOpenVersionModal={setOpenVersionModal}
      selectedImage={undefined}
    />
  );

  expect(screen.queryByRole("dialog")).toBeInTheDocument();

  rerender(
    <ImageVersionModal
      openVersionModal={false}
      setOpenVersionModal={setOpenVersionModal}
      selectedImage={undefined}
    />
  );

  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
