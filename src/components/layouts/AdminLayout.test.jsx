import { render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  fetchData: vi.fn(),
  startClientStatusPolling: vi.fn(),
  stopClientStatusPolling: vi.fn(),
}));

vi.mock("@/store/useAppStore", () => ({
  useAppStore: () => ({
    error: null,
    loading: false,
    fetchData: mocks.fetchData,
    startClientStatusPolling: mocks.startClientStatusPolling,
    stopClientStatusPolling: mocks.stopClientStatusPolling,
  }),
}));
vi.mock("@/components/ui/sidebar", () => ({
  SidebarInset: ({ children }) => <main>{children}</main>,
  SidebarProvider: ({ children }) => <div>{children}</div>,
}));
vi.mock("@/components/ErrorBoundary", () => ({ default: ({ children }) => children }));
vi.mock("@/components/ui/Error", () => ({ Error: () => null }));
vi.mock("@/components/ui/Loading", () => ({ Loading: () => null }));
vi.mock("@/components/ui/ToastContainer", () => ({ ToastContainer: () => null }));
vi.mock("@/components/layouts/Sidebar", () => ({ default: () => null }));
vi.mock("@/components/layouts/Header", () => ({ default: () => null }));
vi.mock("react-router-dom", () => ({
  Outlet: () => null,
  useLocation: () => ({ pathname: "/dashboard" }),
  useNavigate: () => vi.fn(),
  useNavigation: () => ({}),
}));

import AdminLayout from "./AdminLayout";

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});

afterEach(() => localStorage.clear());

it("starts client status polling while mounted and stops it on unmount", () => {
  const { unmount } = render(<AdminLayout />);

  expect(mocks.startClientStatusPolling).toHaveBeenCalledOnce();
  unmount();
  expect(mocks.stopClientStatusPolling).toHaveBeenCalledOnce();
});
