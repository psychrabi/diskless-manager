import { RefreshCw } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Card } from "../ui";

const ServerInfoCard = () => {
  const serverInfo = useAppStore((state) => state.serverInfo);

  return (
    <div>
      {serverInfo ? (
        <div className="card bg-base-100 shadow-xl border border-base-200/50">
          <div className="card-body p-6">
            <h2 className="card-title text-xl mb-4">System Information</h2>
            {serverInfo ? (
              <div className="space-y-4">
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
          </div>
        </div>
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
