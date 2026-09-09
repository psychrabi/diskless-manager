import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import ErrorBoundary from "@/components/ErrorBoundary";
import { Error } from "@/components/ui/Error";
import { Loading } from "@/components/ui/Loading";
import { ToastContainer } from "@/components/ui/ToastContainer";
import { useAppStore } from "@/store/useAppStore";
import { runPreflightCheck } from "@/api/modules/system";
import { useEffect, useRef } from "react";
import {
  Outlet,
  useLocation,
  useNavigate,
  useNavigation,
} from "react-router-dom";

import Sidebar from "@/components/layouts/Sidebar";
import Header from "@/components/layouts/Header";

const AdminLayout = () => {
  const { error, fetchData, loading } = useAppStore();
  const navigation = useNavigation();
  const isNavigating = Boolean(navigation.location);
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // Restore path on mount if we are at root and have a saved path
  const pathRestored = useRef(false);
  useEffect(() => {
    if (pathRestored.current) return;
    const lastPath = localStorage.getItem("last_path");
    if (lastPath && lastPath !== "/" && location.pathname === "/") {
      pathRestored.current = true;
      navigate(lastPath, { replace: true });
    }
  }, [location.pathname, navigate]);

  // Preflight check for setup
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { allServicesInstalled, poolExists } = await runPreflightCheck();

        if (!cancelled && (!allServicesInstalled || !poolExists)) {
          navigate("/setup");
        }
      } catch (e) {
        console.warn("Preflight check failed in AdminLayout:", e);
      }
    })();
    return () => { cancelled = true; };
  }, [location.pathname, navigate]);

  // Save current path after the initial restoration.
  const isFirstRun = useRef(true);
  useEffect(() => {
    if (isFirstRun.current) {
      isFirstRun.current = false;
      return;
    }
    if (location.pathname && location.pathname !== "/login" && location.pathname !== "/setup") {
      localStorage.setItem("last_path", location.pathname);
    }
  }, [location]);

  return (
    <SidebarProvider className="h-svh min-h-0 overflow-hidden">
      <a
        href="#main-content"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById("main-content")?.focus();
        }}
        className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-50 focus:rounded-lg focus:bg-primary focus:px-4 focus:py-2 focus:text-primary-foreground"
      >
        Skip to main content
      </a>
      <Sidebar />
      <SidebarInset className="min-w-0 overflow-hidden md:border">
        <Header />
        <div id="main-content" tabIndex={-1} className="min-h-0 flex-1 overflow-y-auto outline-none">
          <div className="relative mx-auto w-full p-4 md:p-6 lg:p-8">
            {loading && (
              <div className="absolute inset-0 z-40 flex items-center justify-center rounded-lg bg-background/50 backdrop-blur-sm" role="status" aria-live="polite">
                <Loading className="size-10 text-primary" />
                <span className="sr-only">Loading…</span>
              </div>
            )}
            {error && <Error error={error} />}
            {isNavigating ? <Loading /> : (
              <ErrorBoundary fullPage={false} resetKeys={[location.pathname]} onHome={() => navigate("/")}>
                <Outlet />
              </ErrorBoundary>
            )}
          </div>
        </div>
      </SidebarInset>
      <ToastContainer />
    </SidebarProvider>
  );
};

export default AdminLayout;
