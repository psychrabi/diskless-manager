import { Link, useLocation } from "react-router-dom";
import {
  Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList,
  BreadcrumbPage, BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Separator } from "@/components/ui/separator";
import { SidebarTrigger } from "@/components/ui/sidebar";
import ThemeToggle from "./ThemeToggle";
import { getNavigationItem } from "./navigation";
import { useMetrics } from "@/contexts/useMetrics";

export default function Header() {
  const { pathname } = useLocation();
  const current = getNavigationItem(pathname);
  const { isConnected, error } = useMetrics();
  const statusLabel = isConnected ? "Telemetry live" : error ? "Telemetry offline" : "Connecting";
  const statusTone = isConnected
    ? "border-success/25 text-success"
    : error
      ? "border-border text-muted-foreground"
      : "border-border text-muted-foreground";

  return (
    <header className="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border bg-background px-3 sm:px-4 md:px-6">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger className="-ml-1 text-muted-foreground hover:text-foreground" aria-label="Toggle navigation" />
        <Separator orientation="vertical" className="data-vertical:h-4 data-vertical:self-auto" />
        <Breadcrumb className="min-w-0">
          <BreadcrumbList className="flex-nowrap items-center gap-2 text-[13px] sm:gap-2.5 xl:text-sm">
            <BreadcrumbItem className="hidden sm:flex">
              <BreadcrumbLink
                render={<Link to="/" />}
                className="text-[10px] font-semibold tracking-[0.16em] uppercase text-muted-foreground hover:text-foreground"
              >
                {current.section}
              </BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator className="hidden text-border sm:block" />
            <BreadcrumbItem className="min-w-0">
              <BreadcrumbPage className="truncate font-medium text-foreground">{current.label}</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
      </div>
      <div className="flex items-center gap-2">
        <div
          className={`hidden items-center gap-1.5 rounded border px-2 py-1 sm:flex ${statusTone}`}
          role="status"
          aria-live="polite"
          title={isConnected ? "Live telemetry connected" : error ? "Telemetry disconnected" : "Connecting to telemetry"}
        >
          <span
            className={`size-1.5 rounded-full ${isConnected ? "bg-success" : "bg-muted-foreground/60"}`}
            aria-hidden="true"
          />
          <span className="text-[10px] font-semibold tracking-[0.12em] uppercase">{statusLabel}</span>
        </div>
        <ThemeToggle />
      </div>
    </header>
  );
}
