import { useAppStore } from "@/store/useAppStore";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { LoadingSkeleton, CardSkeleton } from "@/components/ui/LoadingSkeleton";
import { Disc, Laptop, MemoryStick, Server, Settings, Activity, TrendingUp } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { Card } from "@/components/ui";
import ClientOverviewCard from "./ClientOverviewCard";
import MasterImageOverviewCard from "./MasterImageOverviewCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import ZfsPoolCard from "./ZfsPoolCard";

export default function Dashboard() {
  const { serverInfo, loading } = useAppStore(
    useShallow((state) => ({
      serverInfo: state.serverInfo,
      loading: state.loading,
    })),
  );

  const serverStatus = serverInfo || {};

  if (loading) {
    return (
      <div className="space-y-6">
        <CardSkeleton showHeader={true} />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          {Array.from({ length: 4 }, (_, i) => (
            <CardSkeleton key={i} showHeader={false} />
          ))}
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
          {Array.from({ length: 5 }, (_, i) => (
            <CardSkeleton key={i} showHeader={true} />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page Header */}
      <Card
        title="System Dashboard"
        subtitle="Monitor your diskless boot server infrastructure and manage connected clients"
        icon={Server}
        variant="elevated"
        className="bg-gradient-to-r from-primary/5 to-secondary/5"
      >
        {/* Key Metrics */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          {/* Services Status */}
          <div className="card-professional bg-gradient-to-br from-success/10 to-success/5 border-success/20">
            <div className="card-body-professional">
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2 mb-2">
                    <div className="w-8 h-8 bg-success/20 rounded-lg flex items-center justify-center">
                      <Settings className="h-4 w-4 text-success" />
                    </div>
                    <span className="text-body-sm font-medium text-base-content/70">Services</span>
                  </div>
                  <div className="text-display-sm font-bold text-base-content">
                    {serverStatus?.services_running ?? 0}
                  </div>
                  <div className="text-body-sm text-base-content/60">
                    of {serverStatus?.services_total ?? 0} running
                  </div>
                </div>
                <StatusBadge
                  status={serverStatus?.services_running === serverStatus?.services_total ? "success" : "warning"}
                  showIcon={false}
                />
              </div>
            </div>
          </div>

          {/* Clients Count */}
          <div className="card-professional bg-gradient-to-br from-info/10 to-info/5 border-info/20">
            <div className="card-body-professional">
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2 mb-2">
                    <div className="w-8 h-8 bg-info/20 rounded-lg flex items-center justify-center">
                      <Laptop className="h-4 w-4 text-info" />
                    </div>
                    <span className="text-body-sm font-medium text-base-content/70">Clients</span>
                  </div>
                  <div className="text-display-sm font-bold text-base-content">
                    {serverStatus?.clients_count ?? 0}
                  </div>
                  <div className="text-body-sm text-base-content/60">
                    Registered devices
                  </div>
                </div>
                <div className="text-info">
                  <TrendingUp className="h-5 w-5" />
                </div>
              </div>
            </div>
          </div>

          {/* Images Count */}
          <div className="card-professional bg-gradient-to-br from-warning/10 to-warning/5 border-warning/20">
            <div className="card-body-professional">
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2 mb-2">
                    <div className="w-8 h-8 bg-warning/20 rounded-lg flex items-center justify-center">
                      <Disc className="h-4 w-4 text-warning" />
                    </div>
                    <span className="text-body-sm font-medium text-base-content/70">Images</span>
                  </div>
                  <div className="text-display-sm font-bold text-base-content">
                    {serverStatus?.images_count ?? 0}
                  </div>
                  <div className="text-body-sm text-base-content/60">
                    Boot images available
                  </div>
                </div>
                <div className="text-warning">
                  <Activity className="h-5 w-5" />
                </div>
              </div>
            </div>
          </div>

          {/* Memory Usage */}
          <div className="card-professional bg-gradient-to-br from-secondary/10 to-secondary/5 border-secondary/20">
            <div className="card-body-professional">
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2 mb-2">
                    <div className="w-8 h-8 bg-secondary/20 rounded-lg flex items-center justify-center">
                      <MemoryStick className="h-4 w-4 text-secondary" />
                    </div>
                    <span className="text-body-sm font-medium text-base-content/70">Memory</span>
                  </div>
                  <div className="text-display-sm font-bold text-base-content">
                    {serverStatus?.memory_usage ?? "0%"}
                  </div>
                  <div className="text-body-sm text-base-content/60">
                    System utilization
                  </div>
                </div>
                <div className="text-secondary">
                  <MemoryStick className="h-5 w-5" />
                </div>
              </div>
            </div>
          </div>
        </div>
      </Card>

      {/* Detailed Overview Cards */}
      <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
        <ServerInfoCard />
        <ServicesStatus />
        <ZfsPoolCard />
        <ClientOverviewCard />
        <MasterImageOverviewCard />
      </div>
    </div>
  );
}
