import { Monitor } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

const InfoRow = ({ label, value }) => (
  <div className="flex items-center justify-between gap-4 py-2">
    <span className="text-sm text-muted-foreground">{label}</span>
    <span className="text-sm font-medium text-right break-all">{value}</span>
  </div>
);

const ServerInfoCard = () => {
  const serverInfo = useAppStore((state) => state.serverInfo);

  return (
    <Card className="h-full xl:col-span-2">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Monitor className="size-4 text-muted-foreground" />
          System Information
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
          <div className="divide-y divide-border">
            <InfoRow label="Hostname" value={serverInfo.hostname} />
            <InfoRow label="Operating System" value={serverInfo.os} />
            <InfoRow label="Kernel" value={serverInfo.kernel} />
            <InfoRow label="Uptime" value={serverInfo.uptime} />
            <InfoRow label="CPU Cores" value={serverInfo.cpu_count} />
            <InfoRow label="Total Memory" value={serverInfo.memory_total} />
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ServerInfoCard;