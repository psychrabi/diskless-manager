import { Card, CardContent } from "@/components/ui/card";
import { useAppStore } from "@/store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";
import { useShallow } from "zustand/react/shallow";

import { Disc, Laptop, MemoryStick } from "lucide-react";

const MetricCard = ({ icon: Icon, label, value, sub }) => (
  <Card className="h-full">
    <CardContent className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <div className="flex size-8 items-center justify-center rounded-lg bg-muted">
          <Icon className="size-4 text-muted-foreground" />
        </div>
        <span className="text-sm font-medium text-muted-foreground">{label}</span>
      </div>
      <div className="text-2xl font-bold tabular-nums">{value}</div>
      <div className="text-sm text-muted-foreground">{sub}</div>
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
    ? liveClients.filter((client) => client.status === "Online").length
    : null;
  const totalClients = serverStatusData?.clients_count ?? liveClients?.length ?? 0;

  return (
    <div className="grid grid-cols-2 gap-4 xl:grid-cols-3">
      <MetricCard
        icon={Laptop}
        label="Clients"
        value={onlineClients ?? totalClients}
        sub={onlineClients != null ? `of ${totalClients} registered` : "Registered devices"}
      />
      <MetricCard
        icon={Disc}
        label="Images"
        value={serverStatusData?.images_count ?? 0}
        sub="Boot images available"
      />
      <MetricCard
        icon={MemoryStick}
        label="Memory"
        value={ramUsage?.percent ? `${ramUsage.percent.toFixed(1)}%` : "0%"}
        sub="System utilization"
      />
    </div>
  );
}
