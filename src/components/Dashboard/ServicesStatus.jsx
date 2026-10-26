import { Activity, RefreshCw } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button } from "@/components/ui/button";
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
    <Card className="h-full min-w-0 md:col-span-2 xl:col-span-6">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Activity className="size-4 text-muted-foreground" />
          Services Status
        </CardTitle>
        <CardAction>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => restartService()}
            title="Restart all services"
          >
            <RefreshCw />
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {servicesError && (
          <div
            role="alert"
            className="mb-3 flex items-center justify-between gap-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm"
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
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-6">
            {services.map((service) => {
              const Icon = getServiceIcon(service.name);
              return (
                <div
                  key={service.name}
                  className="flex min-w-0 items-center justify-between gap-3 rounded-xl bg-muted/50 p-3 transition-colors hover:bg-muted"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="size-9 shrink-0 rounded-lg bg-muted flex items-center justify-center">
                      <Icon className="size-4 text-muted-foreground" />
                    </div>
                    <div className="min-w-0">
                      <p className="font-medium text-sm truncate">
                        {service.display_name}
                      </p>
                      <p className="text-xs text-muted-foreground truncate">
                        {service.name}
                      </p>
                    </div>
                  </div>
                  <StatusDot running={service.running} label={service.display_name} />
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
