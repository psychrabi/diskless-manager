import { useEffect } from "react";
import { isRouteErrorResponse, useLocation, useNavigate, useRouteError } from "react-router-dom";
import ErrorFallback from "./ErrorFallback";
import { clearSavedRoute } from "@/lib/error-recovery";

export default function RouteErrorBoundary({ fullPage = true }) {
  const error = useRouteError();
  const navigate = useNavigate();
  const location = useLocation();
  const notFound = isRouteErrorResponse(error) && error.status === 404;
  useEffect(() => {
    if (!notFound) console.error("[RouteErrorBoundary]", error);
  }, [error, notFound]);

  return (
    <ErrorFallback
      error={error}
      fullPage={fullPage}
      title={notFound ? "Page not found" : undefined}
      description={notFound ? "This page doesn’t exist. Return to the dashboard and choose a page from the navigation." : undefined}
      showDetails={!notFound && import.meta.env.DEV}
      onRetry={notFound ? undefined : () => navigate(`${location.pathname}${location.search}${location.hash}`, { replace: true })}
      onHome={() => { clearSavedRoute(); navigate("/", { replace: true }); }}
    />
  );
}
