import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, Link, RouterProvider } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import AdminLayout from "./AdminLayout";

const state = vi.hoisted(() => ({ error: null, loading: false, fetchData: vi.fn() }));
vi.mock("@/store/useAppStore", () => ({ useAppStore: () => state }));
vi.mock("@/components/ui/sidebar", () => ({
  SidebarProvider: ({ children }) => <div>{children}</div>,
  SidebarInset: ({ children }) => <div>{children}</div>,
}));
vi.mock("@/components/layouts/Sidebar", () => ({ default: () => <Link to="/">Dashboard</Link> }));
vi.mock("@/components/layouts/Header", () => ({ default: () => null }));
vi.mock("@/components/ui/ToastContainer", () => ({ ToastContainer: () => null }));

beforeEach(() => localStorage.clear());
function setup(path) {
  const router = createMemoryRouter([{
    element: <AdminLayout />,
    children: [
      { path: "/", element: <p>Dashboard content</p> },
      { path: "/clients", element: <p>Clients content</p> },
    ],
  }], { initialEntries: [path] });
  render(<RouterProvider router={router} />);
  return router;
}
it("allows Dashboard navigation after opening another page directly", async () => {
  localStorage.setItem("last_path", "/clients");
  const router = setup("/clients");
  expect(await screen.findByText("Clients content")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("link", { name: "Dashboard" }));
  expect(await screen.findByText("Dashboard content")).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/");
});
it("restores the saved page once and then allows Dashboard navigation", async () => {
  localStorage.setItem("last_path", "/clients");
  const router = setup("/");
  await waitFor(() => expect(router.state.location.pathname).toBe("/clients"));
  await userEvent.click(screen.getByRole("link", { name: "Dashboard" }));
  expect(await screen.findByText("Dashboard content")).toBeInTheDocument();
  expect(router.state.location.pathname).toBe("/");
});
