import { RefreshCw } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
    <Card className="col-span-2">
      <CardHeader>
        <CardTitle>Services Status</CardTitle>
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
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {services.map((service) => {
              const Icon = getServiceIcon(service.name);
              return (
                <div
                  key={service.name}
                  className="flex items-center justify-between gap-3 p-3 rounded-xl bg-muted/50 hover:bg-muted transition-colors"
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
                  <Badge variant={service.running ? "outline" : "secondary"}>
                    {service.running ? "Running" : "Stopped"}
                  </Badge>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center h-48 text-muted-foreground">
            <p>No services found</p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}