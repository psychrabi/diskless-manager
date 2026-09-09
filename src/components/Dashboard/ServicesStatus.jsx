import { Activity, RefreshCw } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button } from "@/components/ui/button";
import { StatusDot } from "@/components/ui";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { restartAllServices } from "@/api/modules/services";
import { useConfirm } from "@/contexts/confirmDialog";
import { getServiceIcon } from "@/constants/serviceIcons";

export default function ServicesStatus() {
  const confirm = useConfirm();
  const { services } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      fetchServices: state.fetchServices,
    })),
  );

  async function restartService() {
    await confirm({
      title: "Restart All Services",
      description: "Are you sure you want to restart all services?",
      confirmButtonText: "Restart",
      cancelButtonText: "Cancel",
      onConfirm: () => restartAllServices(),
    });
  }

  return (
    <Card className="h-full md:col-span-2 xl:col-span-6">
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
        {services.length > 0 ? (
          <div className="flex gap-3 overflow-x-auto pb-1">
            {services.map((service) => {
              const Icon = getServiceIcon(service.name);
              return (
                <div
                  key={service.name}
                  className="flex min-w-52 flex-1 items-center justify-between gap-3 rounded-xl bg-muted/50 p-3 transition-colors hover:bg-muted"
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