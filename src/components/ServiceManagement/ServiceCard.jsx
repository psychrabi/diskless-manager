import React from "react";
import { Button, Card } from "../ui";
import {
  Eye,
  Folder,
  FolderOpen,
  FolderOpenDot,
  Globe,
  Network,
  Play,
  RefreshCcw,
  Save,
  Settings,
  StopCircle,
} from "lucide-react";

export default function ServiceCard({
  onViewConfig,
  service,
  startService,
  stopService,
  restartService,
}) {
  function getServiceIcon(name) {
    const icons = {
      "isc-dhcp-server": Network,
      "tftpd-hpa": FolderOpen,
      "rtslib-fb-targetctl": Save,
      "nfs-kernel-server": FolderOpenDot,
      smbd: Folder,
      apache2: Globe,
    };
    return icons[name] || Settings;
  }

  function getServiceDescription(name) {
    const descriptions = {
      "isc-dhcp-server":
        "Provides IP addresses and PXE boot parameters to network clients.",
      "tftpd-hpa":
        "Serves boot files (bootloader, kernel, initrd) via TFTP protocol.",
      target:
        "iSCSI Target (LIO) - serves disk images as network block devices via LIO/ConfigFS.",
      "nfs-kernel-server":
        "Network File System server for sharing filesystems.",
      smbd: "Samba file server for Windows-compatible network file sharing.",
      apache2: "Apache2 HTTP server for serving boot files and iPXE scripts.",
    };
    return descriptions[name] || "System service";
  }

  return (
    <Card
      icon={getServiceIcon(service.name)}
      title={service.display_name}
      subtitle={service.name}
      key={service.name}
      actions={
        <div
          className={`badge rounded-full hidden xl:block ${
            service.running ? "badge-success" : "badge-error"
          } gap-2`}
        >
          {service.running ? "Running" : "Stopped"}
        </div>
      }
    >
      <p className="text-base-content/70 mb-4 min-h-[2.5rem]">
        {getServiceDescription(service.name)}
      </p>

      <div className="flex items-center justify-between text-sm text-base-content/50 mb-6 bg-base-200/30 p-2 rounded-">
        <span>
          PID: <span className="font-mono">{service.pid ?? "—"}</span>
        </span>
        <span
          className={`badge rounded-full ${
            service.enabled ? "badge-success" : "badge-error"
          } badge-sm`}
        >
          {service.enabled ? "Enabled at boot" : "Disabled at boot"}
        </span>
      </div>

      <div className="card-actions">
        {service.running ? (
          <>
            <Button
              icon={RefreshCcw}
              variant="warning"
              className="flex-1"
              onClick={() => restartService(service.name)}
            >
              Restart
            </Button>
            <Button
              icon={StopCircle}
              variant="destructive"
              className="flex-1"
              onClick={() => stopService(service.name)}
            >
              Stop
            </Button>
          </>
        ) : (
          <Button
            icon={Play}
            variant="success"
            className="flex-1"
            onClick={() => startService(service.name)}
          >
            Start
          </Button>
        )}
        <Button
          icon={Eye}
          variant="info"
          className="flex-1"
          onClick={() => onViewConfig(service.name, service.display_name)}
        >
          View Config
        </Button>
      </div>
    </Card>
  );
}
