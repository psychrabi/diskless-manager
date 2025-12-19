import { useAppStore } from "@/store/useAppStore";
import { Disc, Laptop, MemoryStick, Server, Settings } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { Card } from "../ui";
import ClientOverviewCard from "./ClientOverviewCard";
import MasterImageOverviewCard from "./MasterImageOverviewCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import ZfsPoolCard from "./ZfsPoolCard";

export default function Dashboard() {
  const { serverInfo } = useAppStore(
    useShallow((state) => ({
      serverInfo: state.serverInfo,
    }))
  );
  const serverStatus = [];

  return (
    <Card
      title="Dashboard"
      icon={Server}
      subtitle="Diskless Server Management"
      className="bg-base-300"
    >
      {/* Stats Cards */}
      <div className="stats shadow w-full bg-base-100 mb-4">
        <div className="stat">
          <div className="stat-figure text-primary">
            <div className="w-12 h-12 bg-primary/10 rounded-xl flex items-center justify-center text-2xl">
              <Settings />
            </div>
          </div>
          <div className="stat-title">Services</div>
          <div className="stat-value text-primary">
            {serverStatus?.services_running ?? 0}
          </div>
          <div className="stat-desc">
            running out of {serverStatus?.services_total ?? 0} total
          </div>
        </div>

        <div className="stat">
          <div className="stat-figure text-secondary">
            <div className="w-12 h-12 bg-secondary/10 rounded-xl flex items-center justify-center text-2xl">
              <Laptop />
            </div>
          </div>
          <div className="stat-title">Clients</div>
          <div className="stat-value text-secondary">
            {serverStatus?.clients_count ?? 0}
          </div>
          <div className="stat-desc">Registered boot clients</div>
        </div>

        <div className="stat">
          <div className="stat-figure text-accent">
            <div className="w-12 h-12 bg-accent/10 rounded-xl flex items-center justify-center text-2xl">
              <Disc />
            </div>
          </div>
          <div className="stat-title">Images</div>
          <div className="stat-value text-accent">
            {serverStatus?.images_count ?? 0}
          </div>
          <div className="stat-desc">Available boot images</div>
        </div>

        <div className="stat">
          <div className="stat-figure text-success">
            <div className="w-12 h-12 bg-success/10 rounded-xl flex items-center justify-center text-2xl">
              <MemoryStick />
            </div>
          </div>
          <div className="stat-title">Memory</div>
          <div className="stat-value text-success text-2xl">
            {serverInfo?.memory_available ?? "N/A"}
          </div>
          <div className="stat-desc">Available system memory</div>
        </div>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
        {/* System Information */}
        <ServerInfoCard />

        {/* Services Status */}
        <ServicesStatus />

        {/* ZFS Pool */}
        <ZfsPoolCard />

        {/* Client Overview */}
        <ClientOverviewCard />

        {/* Master Image Overview */}
        <MasterImageOverviewCard />
      </div>
    </Card>
  );
}
