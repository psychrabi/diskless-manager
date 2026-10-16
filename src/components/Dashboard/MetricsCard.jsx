import { Card, CardContent } from "@/components/ui/card";
import { useAppStore } from "@/store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";
import { isOnlineStatus } from "@/lib/clientStatus";
import { useShallow } from "zustand/react/shallow";

import { Disc, Laptop, MemoryStick } from "lucide-react";

const MetricCard = ({ icon: Icon, label, value, sub }) => (
  <Card className="h-full min-w-0">
    <CardContent className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-2xl font-semibold text-card-foreground tabular-nums">{value}</div>
        <p className="text-sm text-muted-foreground">{label} <span className="mt-1 text-xs leading-relaxed text-muted-foreground">{sub}</span></p>

      </div>
      <Icon className="size-8 shrink-0 text-primary/60" aria-hidden="true" />
    </CardContent>
  </Card>
);

export default function MetricsCard() {
  const { serverStatus, ramUsage } = useAppStore(
    useShallow((state) => ({
      serverStatus: state.serverStatus,
      ramUsage: state.ramUsage,
    })),
  );

  const serverStatusData = serverStatus || {};

  const { metrics } = useMetrics();
  const liveClients = metrics?.clients;
  const onlineClients = liveClients
    ? liveClients.filter((client) => isOnlineStatus(client.status)).length
    : null;
  const totalClients = serverStatusData?.clients_count ?? liveClients?.length ?? 0;

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
      <MetricCard
        icon={Laptop}
        label="Online"
        value={onlineClients ?? totalClients}
        sub={"client"}
      />
      <MetricCard
        icon={Disc}
        label="Available"
        value={serverStatusData?.images_count ?? 0}
        sub="images"
      />
      <MetricCard
        icon={MemoryStick}
        label="System"
        value={ramUsage?.percent ? `${ramUsage.percent.toFixed(1)}%` : "0%"}
        sub="memory utilization"
      />
    </div>
  );
}
