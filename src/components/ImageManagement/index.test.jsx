import { render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ImageManagement from "./index";
const state = vi.hoisted(() => ({
  masters: [], datasets: [], zpools: ["diskless"],
  fetchMasters: vi.fn(), fetchImages: vi.fn(), fetchDatasets: vi.fn(),
}));
vi.mock("../../store/useAppStore", () => ({ useAppStore: (selector) => selector(state) }));
vi.mock("@/contexts/auth", () => ({ useAuth: () => ({ user: { role: "admin" } }) }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ error: vi.fn(), success: vi.fn() }) }));
vi.mock("react-router-dom", () => ({ Link: ({ children }) => <a>{children}</a> }));
vi.mock("./ImagesList", () => ({ ImagesList: () => null }));
vi.mock("./CreateImageModal", () => ({ default: () => null }));
vi.mock("./WindowsServicingModal", () => ({ default: () => null }));
it("loads image datasets when opened directly", async () => {
  render(<ImageManagement />);
  await waitFor(() => expect(state.fetchDatasets).toHaveBeenCalledWith("diskless"));
  expect(screen.getByRole("button", { name: "Create Image" })).toBeDisabled();
});

it("keeps the image page actions inside a responsive action group", () => {
  const originalMasters = state.masters;
  state.masters = [{ id: "image-1", name: "ubuntu" }];
  const { container } = render(<ImageManagement />);
  const actions = container.querySelector('[data-testid="image-page-actions"]');
  expect(actions).toHaveClass("flex-col", "sm:flex-row");
  expect(screen.getByRole("button", { name: "Windows Servicing" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Import Existing" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Create Image" })).toBeInTheDocument();
  state.masters = originalMasters;
});
