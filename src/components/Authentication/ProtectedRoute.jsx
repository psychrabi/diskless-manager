import { useAuth } from "@/contexts/auth";
import { checkAdminExists } from "@/api/modules/auth";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";

const ProtectedRoute = ({ children }) => {
  const { user, token } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
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
  }, [user, token, navigate]);

  if (user && token) {
    return children;
  }

  return null;
};

export default ProtectedRoute;