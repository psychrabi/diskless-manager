import { CardSkeleton } from "@/components/ui/LoadingSkeleton";
import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/react/shallow";
import ClientOverviewCard from "./ClientOverviewCard";
import MasterImageOverviewCard from "./MasterImageOverviewCard";
import MetricsCard from "./MetricsCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import ZfsPoolCard from "./ZfsPoolCard";

export default function Dashboard() {
  const { loading } = useAppStore(
    useShallow((state) => ({
      loading: state.loading,
    })),
  );

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
      <MetricsCard />

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
