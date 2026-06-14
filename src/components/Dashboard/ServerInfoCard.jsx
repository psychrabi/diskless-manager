import { Monitor, RefreshCw } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Card, Button } from "@/components/ui";
import { clearRamCache } from "@/api/modules/system";

const InfoRow = ({ label, value, className = "" }) => (
  <div className={`flex justify-between items-center py-2.5 border-b border-base-200/50 last:border-0 ${className}`}>
    <span className="text-sm text-base-content/60">{label}</span>
    <span className="text-sm font-medium text-base-content text-right ml-4">{value}</span>
  </div>
);

const ServerInfoCard = () => {
  const serverInfo = useAppStore((state) => state.serverInfo);

  if (!serverInfo) {
    return (
      <Card title="System Information" icon={Monitor}>
        <div className="space-y-3" aria-hidden="true">
          <div className="h-5 bg-base-200 rounded animate-pulse w-full" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-5/6" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-4/5" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-3/4" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-2/3" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-5/6" />
        </div>
      </Card>
    );
  }

  return (
    <Card
      title="System Information"
      icon={Monitor}
      actions={
        <Button
          onClick={clearRamCache}
          variant="ghost"
          size="sm"
          title="Clear RAM cache"
        >
          <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
          Clear Cache
        </Button>
      }
    >
      <div>
        <InfoRow label="Hostname" value={serverInfo.hostname} />
        <InfoRow label="Operating System" value={serverInfo.os} />
        <InfoRow label="Kernel" value={serverInfo.kernel} />
        <InfoRow label="Uptime" value={serverInfo.uptime} />
        <InfoRow label="CPU Cores" value={serverInfo.cpu_count} />
        <InfoRow label="Total Memory" value={serverInfo.memory_total} />
      </div>
    </Card>
  );
};

export default ServerInfoCard;
