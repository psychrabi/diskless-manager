import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import SparePrewarmCard from "./SparePrewarmCard";

const mocks = vi.hoisted(() => ({ getStorageSpares: vi.fn() }));

vi.mock("@/api/modules/system", () => ({
  getStorageSpares: mocks.getStorageSpares,
}));

const view = (overrides = {}) => ({
  enabled: true,
  max_ready_spares: 2,
  ttl_hours: 24,
  min_free_bytes: 50 * 1024 ** 3,
  ready_count: 1,
  spares: [
    {
      client_id: "client",
      client_name: "PC001",
      dataset: "pool/clients/pc001-spare-abc123",
      status: "ready",
      source_snapshot: "pool/master@ready",
      age_seconds: 5400,
      ready: true,
      last_error: null,
      failures: 0,
    },
  ],
  ...overrides,
});

beforeEach(() => {
  vi.resetAllMocks();
});

it("reports ready spares against the configured limit", async () => {
  mocks.getStorageSpares.mockResolvedValue(view());

  render(<SparePrewarmCard />);

  expect(await screen.findByText("PC001")).toBeTruthy();
  expect(screen.getByText(/1 of 2 permitted clones ready/)).toBeTruthy();
  expect(screen.getByText("ready")).toBeTruthy();
  expect(screen.getByText("1h 30m old")).toBeTruthy();
  expect(screen.getByText("pool/clients/pc001-spare-abc123")).toBeTruthy();
});

it("shows repeated build failures and their error", async () => {
  mocks.getStorageSpares.mockResolvedValue(
    view({
      ready_count: 0,
      spares: [
        {
          ...view().spares[0],
          status: "failed",
          ready: false,
          failures: 3,
          last_error: "zfs clone failed: out of space",
        },
      ],
    }),
  );

  render(<SparePrewarmCard />);

  expect(await screen.findByText("failed")).toBeTruthy();
  expect(screen.getByText("3 failed attempts")).toBeTruthy();
  expect(screen.getByText("zfs clone failed: out of space")).toBeTruthy();
});

it("explains when prewarming is disabled", async () => {
  mocks.getStorageSpares.mockResolvedValue(
    view({ enabled: false, ready_count: 0, spares: [] }),
  );

  render(<SparePrewarmCard />);

  // "Disabled" also renders before the payload arrives, so wait for the
  // explanation that only exists once the view has loaded.
  expect(await screen.findByText(/Prewarming is off/)).toBeTruthy();
  expect(screen.getByText("Disabled")).toBeTruthy();
});

it("stays hidden for non-administrators", async () => {
  const error = new Error("Administrator access required");
  error.status = 403;
  mocks.getStorageSpares.mockRejectedValue(error);

  const { container } = render(<SparePrewarmCard />);

  await waitFor(() => expect(mocks.getStorageSpares).toHaveBeenCalled());
  await waitFor(() => expect(container).toBeEmptyDOMElement());
});

it("reports a load failure with a retry", async () => {
  mocks.getStorageSpares.mockRejectedValue(new Error("boom"));

  render(<SparePrewarmCard />);

  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.getByText(/Failed to load prewarmed writebacks: boom/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
});
