import { Monitor } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

const InfoRow = ({ label, value }) => (
  <div className="flex items-baseline justify-between gap-4 border-b border-border/50 py-2 last:border-0">
    <span className="shrink-0 text-[11px] text-muted-foreground">{label}</span>
    <span className="min-w-0 break-all text-right font-mono-data text-[11px] font-medium text-foreground">{value}</span>
  </div>
);

const ServerInfoCard = () => {
  const serverInfo = useAppStore((state) => state.serverInfo);
  const serverStatus = useAppStore((state) => state.serverStatus);

  const iscsiBackendLabel = {
    internal_configfs: "Native configfs",
    targetcli: "targetcli",
  }[serverStatus?.iscsi_backend];

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Monitor className="size-4" />
          </span>
          System
        </CardTitle>
      </CardHeader>
      <CardContent>
        {!serverInfo ? (
          <div className="flex flex-col gap-3" aria-hidden="true">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-4/5" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        ) : (
          <div>
            <InfoRow label="Hostname" value={serverInfo.hostname} />
            <InfoRow label="Operating System" value={serverInfo.os} />
            <InfoRow label="Kernel" value={serverInfo.kernel} />
            <InfoRow label="Uptime" value={serverInfo.uptime} />
            <InfoRow label="CPU Cores" value={serverInfo.cpu_count} />
            <InfoRow label="Total Memory" value={serverInfo.memory_total} />
            {iscsiBackendLabel && (
              <InfoRow label="iSCSI Backend" value={iscsiBackendLabel} />
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ServerInfoCard;
