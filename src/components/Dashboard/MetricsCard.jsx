import { StatCard } from "@/components/ui/StatCard";
import { useAppStore } from "@/store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";
import { isOnlineStatus } from "@/lib/clientStatus";
import { Activity, Disc3, Laptop2, MemoryStick } from "lucide-react";
import { useShallow } from "zustand/react/shallow";

export default function MetricsCard() {
  const { serverStatus, ramUsage, services, clients } = useAppStore(
    useShallow((state) => ({
      serverStatus: state.serverStatus,
      ramUsage: state.ramUsage,
      services: state.services,
      clients: state.clients,
    })),
  );

  const serverStatusData = serverStatus || {};
  const { metrics } = useMetrics();
  const liveClients = metrics?.clients;
  const onlineClients = liveClients
    ? liveClients.filter((client) => isOnlineStatus(client.status)).length
    : clients.filter((client) => isOnlineStatus(client.status)).length;
  const totalClients = serverStatusData?.clients_count ?? liveClients?.length ?? clients.length;
  const online = onlineClients;
  const clientPct = totalClients > 0 ? (online / totalClients) * 100 : 0;

  const images = serverStatusData?.images_count ?? 0;
  const memory = ramUsage?.percent ?? 0;
  const runningServices = services.filter((service) => service.running).length;
  const serviceCount = services.length;
  const servicePct = serviceCount > 0 ? (runningServices / serviceCount) * 100 : 0;

  return (
    <div className="grid grid-cols-2 gap-3 sm:gap-4 lg:grid-cols-4">
      <StatCard
        className="p-3 sm:p-5"
        icon={Laptop2}
        label="Clients online"
        value={online}
        hint={`of ${totalClients} registered`}
        tone="success"
        progress={clientPct}
      />
      <StatCard
        className="p-3 sm:p-5"
        icon={Disc3}
        label="Images available"
        value={images}
        hint="ready for network boot"
        tone="info"
      />
      <StatCard
        className="p-3 sm:p-5"
        icon={MemoryStick}
        label="Memory"
        value={`${Number(memory).toFixed(1)}%`}
        hint="host memory in use"
        tone={memory >= 85 ? "warning" : "primary"}
        progress={memory}
      />
      <StatCard
        className="p-3 sm:p-5"
        icon={Activity}
        label="Services"
        value={serviceCount > 0 ? runningServices : "—"}
        hint={serviceCount > 0 ? `of ${serviceCount} services running` : "No service data"}
        tone={serviceCount > 0 && runningServices === serviceCount ? "success" : "warning"}
        progress={servicePct}
      />
    </div>
  );
}
