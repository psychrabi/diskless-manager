import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { checkDependencies } from "@/api/modules/system";
import { checkZfsPoolExists } from "@/api/modules/disks";
import { getLicenseInfo } from "@/api/modules/license";
import { useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Loading } from "@/components/ui";
import Toast from "../ui/Toast";

const PublicLayout = () => {
  const { setDependencies } = useAppStore();
  const [preflightLoading, setPreflightLoading] = useState(true);
  const navigate = useNavigate();
  const location = useLocation();
  const { toasts, error } = useToastStore();

  // Preflight check before showing login
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await checkDependencies();
        const list = Array.isArray(res) ? res : res ? Object.values(res) : [];
        if (!cancelled) {
          setDependencies(list);
          const allServicesInstalled = list.every((svc) => svc?.installed);
          const poolExists = await checkZfsPoolExists();

          // Only redirect to setup if services are not installed OR pool missing
          // AND we are not already on setup page
          if (
            (!allServicesInstalled || !poolExists) &&
            location.pathname !== "/setup"
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
  }, [navigate, setDependencies, error, location.pathname]);

  if (preflightLoading) {
    return <Loading message="Performing preflight checks..." />;
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-base-200 text-base-content p-4">
      <Outlet />
      <div className="fixed bottom-4 right-4 z-50 space-y-2">
        {toasts.map((toast) => (
          <Toast key={toast.id} toast={toast} />
        ))}
      </div>
    </div>
  );
};

export default PublicLayout;
