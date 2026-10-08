import { useEffect, useState } from "react";
import { Navigate } from "react-router-dom";
import { useAuth } from "@/contexts/auth";
import { getSetupStatus } from "@/api/modules/system";
import { Loading } from "@/components/ui/Loading";
import { Button } from "@/components/ui/button";

export default function SetupGate({ children, setup = false }) {
  const { user, token, loading, logout } = useAuth();
  const [result, setResult] = useState(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (loading || !user || !token) return;
    let cancelled = false;
    getSetupStatus().then(status => {
      if (!cancelled) setResult({ token, attempt, status });
    }).catch(error => {
      if (!cancelled) {
        if (error.status === 401) logout();
        else setResult({ token, attempt, error: true });
      }
    });
    return () => { cancelled = true; };
  }, [loading, user, token, attempt, logout]);
  if (loading) return <Loading message="Validating session…" />;
  if (!user || !token) return <Navigate to="/login" replace />;
  if (!result || result.token !== token || result.attempt !== attempt) return <Loading message="Checking server setup…" />;
  if (result.error) return <div role="alert" className="p-6 space-y-4"><p>Unable to check server setup. Management is unavailable until the check succeeds.</p><Button onClick={() => setAttempt(value => value + 1)}>Retry</Button></div>;
  const complete = result.status?.completed === true && result.status?.ready === true;
  if (!complete && user.role !== "admin") return <div className="p-6 space-y-4"><p>Please ask an administrator to complete server setup.</p><Button onClick={() => setAttempt(value => value + 1)}>Retry</Button></div>;
  if (!setup && !complete) return <Navigate to="/setup" replace />;
  if (setup && complete) return <Navigate to="/" replace />;
  return children;
}
