import { useAuth } from "@/contexts/auth";
import { checkAdminExists } from "@/api/modules/auth";
import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";

const ProtectedRoute = ({ children }) => {
  const { user, token, loading } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    if (loading) return;
    if (user && token && user.role !== "admin" && ["/users", "/license"].includes(location.pathname)) {
      navigate("/", { replace: true });
      return;
    }
    if (user && token) {
      return;
    }

    let cancelled = false;
    const redirectToAppropriateAuthPage = async () => {
      let adminExists;
      try {
        const response = await checkAdminExists();
        adminExists = !!(response.exists || response.admin_exists);
      } catch {
        // API unreachable: keep the secure default of offering login.
        adminExists = true;
      }

      if (!cancelled) {
        navigate(adminExists ? "/login" : "/initial-setup", {
          replace: true,
        });
      }
    };

    redirectToAppropriateAuthPage();
    return () => {
      cancelled = true;
    };
  }, [user, token, loading, navigate, location.pathname]);

  if (!loading && user && token) {
    if (user.role !== "admin" && ["/users", "/license"].includes(location.pathname)) return null;
    return children;
  }

  return null;
};

export default ProtectedRoute;
