import { Activity, RefreshCw } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { StatusDot } from "@/components/ui";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { restartAllServices } from "@/api/modules/services";
import { useConfirm } from "@/contexts/confirmDialog";
import { getServiceIcon } from "@/constants/serviceIcons";
import { useToastStore } from "@/store/useToastStore";

export default function ServicesStatus() {
  const confirm = useConfirm();
  const showError = useToastStore((state) => state.error);
  const { services, servicesError, fetchServices } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      servicesError: state.servicesError,
      fetchServices: state.fetchServices,
    })),
  );
  const running = services.filter((service) => service.running).length;

  async function restartService() {
    const confirmed = await confirm({
      title: "Restart All Services",
      description:
        "Restart every managed service? Active clients may briefly lose connectivity.",
      confirmText: "Restart",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });
    if (!confirmed) return;
    try {
      await restartAllServices();
      await fetchServices();
    } catch (err) {
      showError("Services", err?.message || "Failed to restart services");
    }
  }

  return (
    <Card className="h-full min-w-0">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Activity className="size-4" />
          </span>
          Services
          {services.length > 0 && (
            <Badge variant={running === services.length ? "success" : "warning"}>
              {running}/{services.length}
            </Badge>
          )}
        </CardTitle>
        <CardAction>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => restartService()}
            title="Restart all services"
            aria-label="Restart all services"
          >
            <RefreshCw />
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {servicesError && (
          <div
            role="alert"
            className="mb-3 flex flex-col items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm"
          >
            <span>Failed to load services: {servicesError}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => fetchServices()}
            >
              Retry
            </Button>
          </div>
        )}
        {services.length > 0 ? (
          <div className="grid grid-cols-1 gap-x-5 2xl:grid-cols-2">
            {services.map((service) => {
              const Icon = getServiceIcon(service.name);
              return (
                <div
                  key={service.name}
                  className="flex min-w-0 items-center justify-between gap-3 border-b border-border/60 py-2.5 2xl:[&:nth-last-child(-n+2)]:border-b-0"
                >
                  <div className="flex min-w-0 items-center gap-2.5">
                    <div className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-muted/40">
                      <Icon className="size-3.5 text-muted-foreground" />
                    </div>
                    <div className="min-w-0">
                      <p className="truncate text-[13px] font-medium text-foreground">
                        {service.display_name}
                      </p>
                      <p className="truncate font-mono-data text-[10px] text-muted-foreground">
                        {service.name}
                      </p>
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <span className={service.running ? "text-[11px] text-success" : "text-[11px] text-destructive"}>
                      {service.running ? "Running" : "Stopped"}
                    </span>
                    <StatusDot running={service.running} label={service.display_name} />
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center py-8 text-sm text-muted-foreground">
            <p>No services found</p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
