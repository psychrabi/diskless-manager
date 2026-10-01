import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { ToastContainer } from "@/components/ui/ToastContainer";
import Brand from "./Brand";
import ThemeToggle from "./ThemeToggle";
import ErrorBoundary from "@/components/ErrorBoundary";

const PublicLayout = () => {
  const navigate = useNavigate();
  const location = useLocation();

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
