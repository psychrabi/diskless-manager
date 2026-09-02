import { useToastStore } from "@/store/useToastStore";
import {
  Eye,
  Play,
  RefreshCcw,
  StopCircle,
} from "lucide-react";
import { useState } from "react";
import { Button, Card, StatusBadge } from "@/components/ui";
import { getServiceIcon } from "@/constants/serviceIcons";

const serviceDescriptions = {
  dhcp: "Provides IP addresses and PXE boot parameters to network clients.",
  tftp: "Serves boot files (bootloader, kernel, initrd) via TFTP protocol.",
  iscsi: "iSCSI Target (LIO) — serves disk images as network block devices via LIO/ConfigFS.",
  nfs: "Network File System server for sharing filesystems.",
  samba: "Samba file server for Windows-compatible network file sharing.",
  http: "Apache2 HTTP server for serving boot files and iPXE scripts.",
};

export default function ServiceCard({
  onViewConfig,
  service,
  startService,
  stopService,
  restartService,
}) {
  const { success, error: showError } = useToastStore();
  const [loadingAction, setLoadingAction] = useState(null);

  const handleAction = async (action, fn) => {
    setLoadingAction(action);
    const labels = { start: "started", stop: "stopped", restart: "restarted" };
    try {
      await fn(service.name);
      success(`${service.display_name} ${labels[action] || action} successfully`);
    } catch (e) {
      showError(`Failed to ${action} ${service.display_name}: ${e.message || e}`);
    } finally {
      setLoadingAction(null);
    }
  };

  const Icon = getServiceIcon(service.name);

  return (
    <Card icon={Icon} title={service.display_name} subtitle={service.name}>
      <p className="text-sm text-base-content/70 mb-4 leading-relaxed">
        {serviceDescriptions[service.name] || "System service"}
      </p>

      <div className="flex items-center justify-between px-3 py-2 bg-base-200/50 rounded-lg mb-4 text-sm">
        <span className="text-base-content/50">
          PID: <span className="font-mono text-base-content/70">{service.pid ?? "\u2014"}</span>
        </span>
        <StatusBadge
          status={service.enabled ? "success" : "error"}
          size="sm"
          showIcon={false}
        >
          {service.enabled ? "Enabled at boot" : "Disabled at boot"}
        </StatusBadge>
      </div>

      <div className="flex gap-2">
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
