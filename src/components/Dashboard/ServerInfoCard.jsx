import { RefreshCw } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Card, Button } from "@/components/ui";
import { clearRamCache } from "@/api/modules/system";

const ServerInfoCard = () => {
  const serverInfo = useAppStore((state) => state.serverInfo);

  return (
    <div>
      {serverInfo ? (
        <Card title="System Information" actions={<Button
          onClick={clearRamCache}
          variant="primary"
          className="w-full btn-xs"
        >
          Clear Cache
        </Button>}>

          {serverInfo ? (
            <div className="space-y-1">
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">Hostname</span>
                <span className="font-medium">{serverInfo.hostname}</span>
              </div>
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">Operating System</span>
                <span className="font-medium">{serverInfo.os}</span>
              </div>
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">Kernel</span>
                <span className="font-medium text-sm">
                  {serverInfo.kernel}
                </span>
              </div>
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">Uptime</span>
                <span className="font-medium">{serverInfo.uptime}</span>
              </div>
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">CPU Cores</span>
                <span className="font-medium">{serverInfo.cpu_count}</span>
              </div>
              <div className="flex justify-between border-b border-base-200 pb-2">
                <span className="text-base-content/70">Total Memory</span>
                <span className="font-medium">{serverInfo.memory_total}</span>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-48 text-base-content/50">
              <span className="loading loading-dots loading-md mb-2"></span>
              <p>System information unavailable</p>
            </div>
          )}
        </Card>
      ) : (
        <Card title="Server Info" icon={RefreshCw}>
          <div className="text-center py-4 text-gray-500">
            Loading server info...
          </div>
        </Card>
      )}
    </div>
  );
};

export default ServerInfoCard;
