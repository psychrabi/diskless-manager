import { useToastStore } from "@/store/useToastStore";
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
import { useState } from "react";
import { Button, Card } from "@/components/ui";

export default function ServiceCard({
  onViewConfig,
  service,
  startService,
  stopService,
  restartService,
}) {
  const { success, error: showError } = useToastStore();
  const [loadingAction, setLoadingAction] = useState(null);

  function getServiceIcon(name) {
    const icons = {
      "dhcp": Network,
      "tftp": FolderOpen,
      "iscsi": Save,
      "nfs": FolderOpenDot,
      "samba": Folder,
      "http": Globe,
    };
    return icons[name] || Settings;
  }

  function getServiceDescription(name) {
    const descriptions = {
      "dhcp":
        "Provides IP addresses and PXE boot parameters to network clients.",
      "tftp":
        "Serves boot files (bootloader, kernel, initrd) via TFTP protocol.",
      'iscsi':
        "iSCSI Target (LIO) - serves disk images as network block devices via LIO/ConfigFS.",
      "nfs":
        "Network File System server for sharing filesystems.",
      "samba": "Samba file server for Windows-compatible network file sharing.",
      "http": "Apache2 HTTP server for serving boot files and iPXE scripts.",
    };
    return descriptions[name] || "System service";
  }

  const handleAction = async (action, fn) => {
    setLoadingAction(action);
    const labels = {
      start: "started",
      stop: "stopped",
      restart: "restarted",
    };
    try {
      await fn(service.name);
      success(
        `${service.display_name} ${labels[action] || action} successfully`
      );
    } catch (e) {
      showError(
        `Failed to ${action} ${service.display_name}: ${e.message || e}`
      );
    } finally {
      setLoadingAction(null);
    }
  };

  return (
    <Card
      icon={getServiceIcon(service.name)}
      title={service.display_name}
      subtitle={service.name}
      key={service.name}
      actions={
        <div
          className={`badge rounded-full hidden xl:block ${service.running ? "badge-success" : "badge-error"
            } gap-2`}
        >
          {service.running ? "Running" : "Stopped"}
        </div>
      }
    >
      <p className="text-base-content/70 mb-4 min-h-[2.5rem]">
        {getServiceDescription(service.name)}
      </p>

      <div className="flex items-center justify-between text-sm text-base-content/50 mb-6 bg-base-200/30">
        <span>
          PID: <span className="font-mono">{service.pid ?? "—"}</span>
        </span>
        <span
          className={`badge rounded-full ${service.enabled ? "badge-success" : "badge-error"
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
              loading={loadingAction === "restart"}
              onClick={() => handleAction("restart", restartService)}
            >
              Restart
            </Button>
            <Button
              icon={StopCircle}
              variant="destructive"
              className="flex-1"
              loading={loadingAction === "stop"}
              onClick={() => handleAction("stop", stopService)}
            >
              Stop
            </Button>
          </>
        ) : (
          <Button
            icon={Play}
            variant="success"
            className="flex-1"
            loading={loadingAction === "start"}
            onClick={() => handleAction("start", startService)}
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
