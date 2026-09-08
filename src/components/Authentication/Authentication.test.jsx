import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Login from "./Login";
import InitialSetup from "./InitialSetup";

const mocks = vi.hoisted(() => ({ login: vi.fn(), bootstrapAdmin: vi.fn(), checkAdminExists: vi.fn(), setAuth: vi.fn(), error: vi.fn(), success: vi.fn() }));
vi.mock("@/api/modules/auth", () => mocks);
vi.mock("@/contexts/auth", () => ({ useAuth: () => ({ login: mocks.setAuth }) }));
vi.mock("@/store/useToastStore", () => ({ useToastStore: () => ({ error: mocks.error, success: mocks.success }) }));

function renderAuth(path = "/login") {
  return render(<MemoryRouter initialEntries={[path]}><Routes>
    <Route path="/login" element={<Login />} />
    <Route path="/initial-setup" element={<InitialSetup />} />
    <Route path="/" element={<h1>Dashboard</h1>} />
  </Routes></MemoryRouter>);
}

describe("authentication blocks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.checkAdminExists.mockResolvedValue({ exists: true });
    mocks.login.mockResolvedValue({ user: { username: "admin" }, token: "test-token" });
    mocks.bootstrapAdmin.mockResolvedValue({});
  });

  it("submits username credentials and opens the authenticated workspace", async () => {
    const user = userEvent.setup();
    renderAuth();
    await user.type(screen.getByLabelText("Username"), "admin");
    await user.type(screen.getByLabelText("Password"), "Testpass123");
    await user.click(screen.getByRole("button", { name: "Sign in", exact: true }));
    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(mocks.login).toHaveBeenCalledWith("admin", "Testpass123");
    expect(mocks.setAuth).toHaveBeenCalledWith({ username: "admin" }, "test-token");
  });

  it("shows first-admin setup only when the server reports no administrator", async () => {
    mocks.checkAdminExists.mockResolvedValue({ exists: false });
    renderAuth();
    await userEvent.setup().click(await screen.findByRole("button", { name: "Create your first admin account" }));
    expect(screen.getByRole("heading", { name: "Set up your workspace" })).toBeInTheDocument();
  });

  it("validates password confirmation before creating and signing in the administrator", async () => {
    const user = userEvent.setup();
    renderAuth("/initial-setup");
    await user.type(screen.getByLabelText("Password", { exact: true }), "Testpass123");
    await user.type(screen.getByLabelText("Confirm password"), "Different123");
    await user.click(screen.getByRole("button", { name: "Create admin account" }));
    expect(await screen.findByText("Passwords don't match")).toBeInTheDocument();
    expect(mocks.bootstrapAdmin).not.toHaveBeenCalled();
    await user.clear(screen.getByLabelText("Confirm password"));
    await user.type(screen.getByLabelText("Confirm password"), "Testpass123");
    await user.click(screen.getByRole("button", { name: "Create admin account" }));
    await waitFor(() => expect(mocks.bootstrapAdmin).toHaveBeenCalledWith("admin", "Testpass123"));
    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
  });
});
