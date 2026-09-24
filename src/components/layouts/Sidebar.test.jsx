import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import Sidebar from "./Sidebar";
import Header from "./Header";

const { logout, setTheme } = vi.hoisted(() => ({ logout: vi.fn(), setTheme: vi.fn() }));
vi.mock("@/contexts/auth", () => ({ useAuth: () => ({ user: { username: "operator", role: "user" }, logout }) }));
vi.mock("@/contexts/theme", () => ({ useTheme: () => ({ theme: "light", setTheme }) }));
vi.mock("@/contexts/confirmDialog", () => ({ useConfirm: () => vi.fn() }));

function Workspace({ path = "/clients" }) {
  return (
    <MemoryRouter initialEntries={[path]}>
      <SidebarProvider>
        <Sidebar />
        <SidebarInset><Header /></SidebarInset>
      </SidebarProvider>
    </MemoryRouter>
  );
}

describe("sidebar-07 workspace", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    window.matchMedia.mockImplementation((query) => ({
      matches: false, media: query, addEventListener: vi.fn(), removeEventListener: vi.fn(),
    }));
  });

  it("keeps navigation and breadcrumbs in sync and remembers desktop collapse", async () => {
    const user = userEvent.setup();
    const { container, unmount } = render(<Workspace />);
    const nav = within(screen.getByRole("navigation", { name: "Main navigation" }));
    expect(nav.getByRole("link", { name: "Clients" })).toHaveAttribute("aria-current", "page");
    expect(nav.getByRole("link", { name: "Dashboard" })).not.toHaveAttribute("aria-current");

    await user.click(nav.getByRole("link", { name: "Images" }));
    expect(within(screen.getByRole("navigation", { name: "breadcrumb" })).getByText("Images")).toHaveAttribute("aria-current", "page");
    await user.click(screen.getByRole("button", { name: "Toggle navigation" }));
    expect(container.querySelector('[data-slot="sidebar"]')).toHaveAttribute("data-state", "collapsed");
    expect(localStorage.getItem("diskless:sidebar-open")).toBe("false");
    unmount();

    const next = render(<Workspace />);
    expect(next.container.querySelector('[data-slot="sidebar"]')).toHaveAttribute("data-state", "collapsed");
    await user.keyboard("{Control>}b{/Control}");
    expect(localStorage.getItem("diskless:sidebar-open")).toBe("true");
  });

  it("opens mobile navigation and closes it after following a route", async () => {
    window.matchMedia.mockImplementation((query) => ({
      matches: query === "(max-width: 767px)", media: query,
      addEventListener: vi.fn(), removeEventListener: vi.fn(),
    }));
    const user = userEvent.setup();
    render(<Workspace />);
    expect(screen.queryByRole("navigation", { name: "Main navigation" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Toggle navigation" }));
    const dialog = await screen.findByRole("dialog", { name: "Navigation" });
    await user.click(within(dialog).getByRole("link", { name: "Disks" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(screen.getByRole("navigation", { name: "breadcrumb" })).getByText("Disks")).toHaveAttribute("aria-current", "page");
    expect(localStorage.getItem("diskless:sidebar-open")).toBeNull();
  });

  it("uses the real account actions and theme toggle", async () => {
    const user = userEvent.setup();
    render(<Workspace />);
    await user.click(screen.getByRole("button", { name: "Switch to dark mode" }));
    expect(setTheme).toHaveBeenCalledWith("dark");
    await user.click(screen.getByRole("button", { name: "Account menu for operator" }));
    await user.click(await screen.findByRole("menuitem", { name: "Sign out" }));
    expect(logout).toHaveBeenCalledOnce();
  });
});
