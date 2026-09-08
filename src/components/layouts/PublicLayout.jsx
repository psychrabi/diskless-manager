import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useAuth } from "@/contexts/auth";
import { runPreflightCheck } from "@/api/modules/system";
import { getLicenseInfo } from "@/api/modules/license";
import { useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Loading, ToastContainer } from "@/components/ui";

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
    <div className="min-h-screen flex items-center justify-center bg-gradient-to-br from-primary/5 via-base-200 to-secondary/5 text-base-content p-4">
      <Outlet />
      <ToastContainer />
    </div>
  );
};

export default PublicLayout;
