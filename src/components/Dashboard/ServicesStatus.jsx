import { RefreshCw } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card, StatusBadge } from "@/components/ui";
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
    <Card
      title="Services Status"
      className="col-span-2"
      actions={
        <Button
          variant="ghost"
          size="icon"
          icon={RefreshCw}
          onClick={() => restartService()}
          title="Refresh all services"
        />
      }
    >
      {services.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {services.map((service) => {
            const Icon = getServiceIcon(service.name);
            return (
              <div
                key={service.name}
                className="flex items-center justify-between p-3 bg-base-200/50 rounded-xl hover:bg-base-200 transition-colors"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-9 h-9 bg-base-300/50 rounded-lg flex items-center justify-center shrink-0">
                    <Icon className="w-4 h-4 text-base-content/70" />
                  </div>
                  <div className="min-w-0">
                    <p className="font-medium text-sm text-base-content truncate">
                      {service.display_name}
                    </p>
                    <p className="text-xs text-base-content/50 truncate">
                      {service.name}
                    </p>
                  </div>
                </div>
                <StatusBadge
                  status={service.running ? "running" : "stopped"}
                  size="sm"
                  showIcon={false}
                />
              </div>
            );
          })}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center h-48 text-base-content/50">
          <p>No services found</p>
        </div>
      )}
    </Card>
  );
}
