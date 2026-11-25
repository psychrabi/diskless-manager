import { CircleAlert } from "lucide-react";
import { useMetrics } from "@/contexts/useMetrics";
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
  const { error } = useMetrics();
  const { loading } = useAppStore(
    useShallow((state) => ({
      loading: state.loading,
    })),
  );

  if (loading) {
    return (
      <div className="flex flex-col gap-5" aria-busy="true" aria-label="Loading dashboard">
        <div className="mb-1 space-y-2 border-b border-border pb-5 sm:mb-3 sm:pb-5">
          <Skeleton className="h-8 w-48 sm:h-9 sm:w-56" />
          <Skeleton className="h-4 w-full max-w-md" />
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="h-32 rounded-xl" />
          ))}
        </div>
        <div className="grid grid-cols-1 gap-5 xl:grid-cols-12">
          <Skeleton className="h-72 rounded-xl xl:col-span-8" />
          <Skeleton className="h-72 rounded-xl xl:col-span-4" />
          <Skeleton className="h-80 rounded-xl xl:col-span-5" />
          <Skeleton className="h-80 rounded-xl xl:col-span-7" />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Dashboard"
        description="Clients, storage, and the services behind every boot."
      />
      {error && (
        <div role="status" className="flex items-start gap-3 rounded-xl border border-warning/25 bg-warning/8 px-4 py-3 text-sm">
          <CircleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
          <p><strong>Live telemetry unavailable.</strong> Client and storage activity may be out of date.</p>
        </div>
      )}
      <MetricsCard />
      <div className="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,1.45fr)_minmax(0,1fr)]">
        <div className="flex min-w-0 flex-col gap-5">
          <AlarmInbox />
          <StorageCard />
        </div>
        <div className="flex min-w-0 flex-col gap-5">
          <ServicesStatus />
          <StorageMaintenanceCard />
        </div>
      </div>

      <div className="grid grid-cols-1 items-start gap-5 xl:grid-cols-12">
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
