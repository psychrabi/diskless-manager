import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/react/shallow";

import { Activity, Disc, Laptop, MemoryStick, Server, Settings } from "lucide-react";

const MetricCard = ({ icon: Icon, label, value, sub, badge }) => (
  <Card>
    <CardContent className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <div className="flex size-8 items-center justify-center rounded-lg bg-muted">
            <Icon className="size-4 text-muted-foreground" />
          </div>
          <span className="text-sm font-medium text-muted-foreground">{label}</span>
        </div>
        {badge}
      </div>
      <div className="text-2xl font-bold">{value}</div>
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
  const servicesRunning = serverStatusData?.services_running ?? 0;
  const servicesTotal = serverStatusData?.services_total ?? 0;
  const allRunning = servicesRunning === servicesTotal;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Server className="size-4" />
          System Dashboard
        </CardTitle>
        <CardDescription>
          Monitor your diskless boot server infrastructure and manage connected clients
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <MetricCard
            icon={Settings}
            label="Services"
            value={servicesRunning}
            sub={`of ${servicesTotal} running`}
            badge={
              <Badge variant={allRunning ? "outline" : "secondary"}>
                {allRunning ? "Healthy" : "Degraded"}
              </Badge>
            }
          />
          <MetricCard
            icon={Laptop}
            label="Clients"
            value={serverStatusData?.clients_count ?? 0}
            sub="Registered devices"
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
      </CardContent>
    </Card>
  );
}