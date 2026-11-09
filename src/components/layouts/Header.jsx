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
  return (
    <header className="flex h-16 shrink-0 items-center justify-between gap-3 border-b border-border/70 bg-background/75 px-4 backdrop-blur-xl md:px-6">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger className="-ml-1 text-muted-foreground hover:text-foreground" aria-label="Toggle navigation" />
        <Separator orientation="vertical" className="data-vertical:h-4 data-vertical:self-auto" />
        <Breadcrumb className="min-w-0">
          <BreadcrumbList className="flex-nowrap text-[13px]">
            <BreadcrumbItem className="hidden sm:flex">
              <BreadcrumbLink render={<Link to="/" />} className="text-muted-foreground hover:text-foreground">Workspace</BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator className="hidden text-muted-foreground/40 sm:block" />
            <BreadcrumbItem className="min-w-0">
              <BreadcrumbPage className="truncate font-medium text-foreground">{current.label}</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
      </div>
      <div className="flex items-center gap-2">
        <div className="hidden items-center gap-2 px-1.5 text-[11px] font-medium text-muted-foreground sm:flex" role="status" aria-live="polite">
          <span className="relative flex size-1.5">
            <span className={`relative inline-flex size-1.5 rounded-full ${isConnected ? "bg-success" : "bg-muted-foreground/50"}`} />
          </span>
          {isConnected ? "Telemetry connected" : error ? "Telemetry disconnected" : "Connecting to telemetry"}
        </div>
        <ThemeToggle />
      </div>
    </header>
  );
}
