import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/ui";
import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/react/shallow";
import ArcCacheCard from "./ArcCacheCard";
import AlarmInbox from "./AlarmInbox";
import MetricsCard from "./MetricsCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import SparePrewarmCard from "./SparePrewarmCard";
import StorageCard from "./StorageCard";
import StorageMaintenanceCard from "./StorageMaintenanceCard";

export default function Dashboard() {
  const { loading } = useAppStore(
    useShallow((state) => ({
      loading: state.loading,
    })),
  );

  if (loading) {
    return (
      <div className="flex flex-col gap-5">
        <div className="space-y-2">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-8 w-56" />
          <Skeleton className="h-4 w-96 max-w-full" />
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="h-32 rounded-2xl" />
          ))}
        </div>
        <div className="grid grid-cols-1 gap-5 xl:grid-cols-12">
          <Skeleton className="h-72 rounded-2xl xl:col-span-8" />
          <Skeleton className="h-72 rounded-2xl xl:col-span-4" />
          <Skeleton className="h-80 rounded-2xl xl:col-span-5" />
          <Skeleton className="h-80 rounded-2xl xl:col-span-7" />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Server overview"
        description="Clients, storage, and the services behind every boot."
      />
      <MetricsCard />
      <div className="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,1.45fr)_minmax(0,1fr)]">
        <AlarmInbox />
        <ServicesStatus />
      </div>

      <div className="grid grid-cols-1 items-start gap-5 xl:grid-cols-12">
        <div className="xl:col-span-5">
          <StorageCard />
        </div>
        <div className="xl:col-span-7">
          <StorageMaintenanceCard />
        </div>
        <div className="xl:col-span-4">
          <ArcCacheCard />
        </div>
        <div className="xl:col-span-4">
          <ServerInfoCard />
        </div>
        <div className="xl:col-span-4">
          <SparePrewarmCard />
        </div>
      </div>

    </div>
  );
}
