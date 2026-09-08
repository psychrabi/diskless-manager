import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useAuth } from "@/contexts/auth";
import { runPreflightCheck } from "@/api/modules/system";
import { getLicenseInfo } from "@/api/modules/license";
import { useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Loading } from "@/components/ui/Loading";
import { ToastContainer } from "@/components/ui/ToastContainer";
import Brand from "./Brand";
import ThemeToggle from "./ThemeToggle";
import ErrorBoundary from "@/components/ErrorBoundary";

const PublicLayout = () => {
  const { setDependencies } = useAppStore();
  const { token } = useAuth();
  const [preflightLoading, setPreflightLoading] = useState(() =>
    Boolean(token),
  );
  const navigate = useNavigate();
  const location = useLocation();
  const { error } = useToastStore();

  // Preflight check before showing login
  useEffect(() => {
    let cancelled = false;

    // Preflight and license checks require authentication. Skip them
    // when not logged in so the login/setup screens do not fire 401
    // error toasts on first load.
    if (!token) return;

    (async () => {
      try {
        const { list, allServicesInstalled, poolExists } =
          await runPreflightCheck();
        if (!cancelled) {
          setDependencies(list);
          // Only redirect to setup if services are not installed OR pool missing
          // AND we are not already on setup or the initial admin page
          if (
            (!allServicesInstalled || !poolExists) &&
            location.pathname !== "/setup" &&
            location.pathname !== "/initial-setup"
          ) {
            navigate("/setup");
          }
        }
      } catch (e) {
        error(
          `Preflight Check Failed : ${e.message || "An unknown error occurred."
          }`,
        );
        console.warn("Preflight check failed:", e);
        // Proceed to login UI even if preflight fails
      } finally {
        if (!cancelled) setPreflightLoading(false);
      }

      try {
        await getLicenseInfo();
      } catch (err) {
        error(
          `License Check Failed : ${err.message || "An unknown error occurred."
          }`,
        );
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [navigate, setDependencies, error, location.pathname, token]);

  if (preflightLoading) {
    return <Loading message="Performing preflight checks..." />;
  }

  return (
    <div className="flex min-h-svh flex-col bg-muted/40 text-foreground">
      <header className="flex h-16 shrink-0 items-center justify-between px-6 md:px-10">
        {location.pathname === "/setup" ? <Brand titleClassName="text-sm" /> : <span className="text-xs font-medium text-muted-foreground">Boot server workspace</span>}
        <ThemeToggle />
      </header>
      <main className={location.pathname === "/setup" ? "mx-auto w-full max-w-5xl flex-1 px-4 py-8 md:px-8" : "flex flex-1 items-center justify-center px-4 py-8 sm:px-6 md:pb-20"}>
        <ErrorBoundary fullPage={false} resetKeys={[location.pathname]} onHome={() => navigate("/")}>
        {location.pathname === "/setup" ? <Outlet /> : (
          <div className="flex w-full max-w-sm flex-col items-center gap-7">
            <Brand titleClassName="text-base" subtitle="Diskless boot server management" />
            <Outlet />
          </div>
        )}
        </ErrorBoundary>
      </main>
      <ToastContainer />
    </div>
  );
};

export default PublicLayout;
