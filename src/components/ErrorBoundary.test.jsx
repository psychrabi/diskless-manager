import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryRouter, Outlet, RouterProvider } from "react-router-dom";
import ErrorBoundary from "./ErrorBoundary";
import ErrorFallback from "./ErrorFallback";
import RouteErrorBoundary from "./RouteErrorBoundary";

function BrokenPage({ broken = true }) {
  if (broken) throw new Error("Test rendering failure");
  return <p>Page recovered</p>;
}

describe("application error recovery", () => {
  beforeEach(() => { vi.spyOn(console, "error").mockImplementation(() => {}); });
  afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });

  it("renders a provider-independent fallback, focuses its heading, and retries the page", async () => {
    let broken = true;
    const Page = () => <BrokenPage broken={broken} />;
    const user = userEvent.setup();
    render(<><nav>Workspace navigation</nav><ErrorBoundary fullPage={false} showDetails={false}><Page /></ErrorBoundary></>);
    expect(screen.getByText("Workspace navigation")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Something went wrong" })).toHaveFocus();
    expect(screen.queryByText("Test rendering failure")).not.toBeInTheDocument();
    broken = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("Page recovered")).toBeInTheDocument();
  });

  it("recovers automatically when navigation changes the reset keys", () => {
    const { rerender } = render(<ErrorBoundary resetKeys={["/clients"]}><BrokenPage /></ErrorBoundary>);
    rerender(<ErrorBoundary resetKeys={["/images"]}><BrokenPage broken={false} /></ErrorBoundary>);
    expect(screen.getByText("Page recovered")).toBeInTheDocument();
  });

  it("clears the failed saved route when returning home without clearing the session", async () => {
    const onHome = vi.fn();
    localStorage.setItem("last_path", "/clients");
    localStorage.setItem("authToken", "test-session");
    render(<ErrorBoundary onHome={onHome}><BrokenPage /></ErrorBoundary>);
    await userEvent.setup().click(screen.getByRole("button", { name: "Go to dashboard" }));
    expect(onHome).toHaveBeenCalledOnce();
    expect(localStorage.getItem("last_path")).toBeNull();
    expect(localStorage.getItem("authToken")).toBe("test-session");
  });

  it("handles thrown null and offers reload for failed lazy modules", () => {
    function NullError() { throw null; }
    const { unmount } = render(<ErrorBoundary><NullError /></ErrorBoundary>);
    expect(screen.getByRole("heading", { name: "Something went wrong" })).toBeInTheDocument();
    unmount();
    render(<ErrorFallback error={new Error("Failed to fetch dynamically imported module")} onRetry={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "This page could not load" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reload application" })).toBeInTheDocument();
  });

  it("keeps the layout available during route loader failures and retries them", async () => {
    let failing = true;
    const router = createMemoryRouter([{
      element: <><nav>Server navigation</nav><Outlet /></>,
      children: [{
        path: "/clients",
        loader: () => { if (failing) throw new Error("Loader failed"); return null; },
        element: <p>Client list loaded</p>,
        errorElement: <RouteErrorBoundary fullPage={false} />,
      }],
    }], { initialEntries: ["/clients"] });
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole("heading", { name: "Something went wrong" })).toBeInTheDocument();
    expect(screen.getByText("Server navigation")).toBeInTheDocument();
    failing = false;
    await userEvent.setup().click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Client list loaded")).toBeInTheDocument();
    router.dispose();
  });

  it("shows an actionable 404 and navigates back to the dashboard", async () => {
    const router = createMemoryRouter([
      { path: "/", element: <h1>Dashboard</h1> },
      { path: "*", loader: () => { throw new Response(null, { status: 404 }); }, errorElement: <RouteErrorBoundary /> },
    ], { initialEntries: ["/missing"] });
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole("heading", { name: "Page not found" })).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Go to dashboard" }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "Dashboard" })).toBeInTheDocument());
    router.dispose();
  });
});
