import { useToastStore } from "@/store/useToastStore";
import {
  Eye,
  Play,
  RefreshCcw,
  StopCircle,
  ToggleLeft,
  ToggleRight,
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
  enableServiceBoot,
  disableServiceBoot,
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

  const handleBootToggle = async () => {
    const action = service.starts_on_boot ? "disable_boot" : "enable_boot";
    const fn = service.starts_on_boot ? disableServiceBoot : enableServiceBoot;
    const label = service.starts_on_boot
      ? "will no longer start on boot"
      : "will now start on boot";
    setLoadingAction(action);
    try {
      await fn(service.name);
      success(`${service.display_name} ${label}`);
    } catch (e) {
      showError(`Failed to change boot setting for ${service.display_name}: ${e.message || e}`);
    } finally {
      setLoadingAction(null);
    }
  };

  const Icon = getServiceIcon(service.name);

  return (
    <Card icon={Icon} title={service.display_name} subtitle={service.name}>
      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
        {serviceDescriptions[service.name] || "System service"}
      </p>

      <div className="flex items-center justify-between px-3 py-2 bg-muted/50 rounded-lg mb-4 text-sm">
        <span className="text-muted-foreground">
          PID: <span className=" text-muted-foreground">{service.pid ?? "\u2014"}</span>
        </span>
        <StatusBadge
          status={service.starts_on_boot ? "success" : "neutral"}
          size="sm"
          showIcon={false}
        >
          {service.starts_on_boot ? "Starts on boot" : "Manual start"}
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
      <Button
        icon={service.starts_on_boot ? ToggleRight : ToggleLeft}
        variant="ghost"
        className="w-full mt-2"
        loading={loadingAction === "enable_boot" || loadingAction === "disable_boot"}
        onClick={handleBootToggle}
        title={service.starts_on_boot ? "Stop starting on boot" : "Start on boot"}
      >
        {service.starts_on_boot ? "On boot: on" : "On boot: off"}
      </Button>
    </Card>
  );
}
