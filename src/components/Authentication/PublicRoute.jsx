import { useAuth } from "@/contexts/auth";
import { checkAdminExists } from "@/api/modules/auth";
import { useEffect, useState } from "react";
import { Navigate, useLocation } from "react-router-dom";
import { Loading } from "@/components/ui/Loading";
import { Button } from "@/components/ui/button";

const PublicRoute = ({ children }) => {
  const { user, token, loading } = useAuth();
  const { pathname } = useLocation();
  const [result, setResult] = useState(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (loading || (user && token)) return;
    let cancelled = false;
    checkAdminExists().then(response => {
      if (!cancelled) setResult({ pathname, attempt, exists: Boolean(response.exists ?? response.admin_exists) });
    }).catch(() => { if (!cancelled) setResult({ pathname, attempt, error: true }); });
    return () => { cancelled = true; };
  }, [loading, user, token, pathname, attempt]);
  if (loading) return <Loading message="Validating session…" />;
  if (user && token) return <Navigate to="/" replace />;
  if (!result || result.pathname !== pathname || result.attempt !== attempt) return <Loading message="Checking administrator account…" />;
  if (result.error) return <div role="alert"><p>Unable to check administrator account.</p><Button onClick={() => setAttempt(value => value + 1)}>Retry</Button></div>;
  const exists = result.exists;
  if (!exists && pathname !== "/initial-setup") return <Navigate to="/initial-setup" replace />;
  if (exists && pathname === "/initial-setup") return <Navigate to="/login" replace />;
  return children;
};
export default PublicRoute;
