import { Folder, Globe, RefreshCw, Save, Settings } from "lucide-react";
import React from "react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card } from "../ui";
import { restartAllServices } from "@/api/commands";
import { useConfirm } from "@/contexts/confirmDialog";

export default function ServicesStatus() {
  const confirm = useConfirm();
  const { services } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      fetchServices: state.fetchServices,
    })),
  );

  function getServiceIcon(name) {
    const icons = {
      "isc-dhcp-server": Globe,
      "tftpd-hpa": Folder,
      target: Save,
      "nfs-kernel-server": Folder,
      smbd: Folder,
      apache2: Globe,
    };
    return icons[name] || Settings;
  }

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
      <div className="">
        {services.length > 0 ? (
          <ul className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {services.map((service) => (
              <li
                key={service.name}
                className="flex items-center justify-between p-3 bg-base-200 rounded-lg hover:bg-base-200 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <span className="text-xl opacity-80">
                    {React.createElement(getServiceIcon(service.name))}
                  </span>
                  <div>
                    <p className="font-medium text-base-content">
                      {service.display_name}
                    </p>
                    <p className="text-xs text-base-content/60">
                      {service.name}
                    </p>
                  </div>
                </div>
                <span
                  className={`badge ${service.running ? "badge-success" : "badge-error"
                    } badge-sm font-semibold`}
                >
                  {service.running ? "Running" : "Stopped"}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <div className="flex flex-col items-center justify-center h-48 text-base-content/50">
            <p>No services found</p>
          </div>
        )}
      </div>
    </Card>
  );
}
