import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/ui";
import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/react/shallow";
import ArcCacheCard from "./ArcCacheCard";
import MetricsCard from "./MetricsCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import StorageCard from "./StorageCard";

export default function Dashboard() {
  const { loading } = useAppStore(
    useShallow((state) => ({
      loading: state.loading,
    })),
  );

  if (loading) {
    return (
      <div className="flex flex-col gap-4">
        <div className="space-y-1">
          <Skeleton className="h-7 w-48" />
          <Skeleton className="h-4 w-96 max-w-full" />
        </div>
        <div className="grid grid-cols-2 gap-4 xl:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))}
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-6">
          <Skeleton className="h-48 rounded-xl xl:col-span-2" />
          <Skeleton className="h-48 rounded-xl xl:col-span-2" />
          <Skeleton className="h-48 rounded-xl xl:col-span-2" />
          <Skeleton className="h-32 rounded-xl md:col-span-2 xl:col-span-6" />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Server overview"
        description="Monitor clients, storage, and the services behind your boot environment."
      />
      <MetricsCard />

      <div className="grid grid-cols-1 items-stretch gap-4 md:grid-cols-2 xl:grid-cols-6">
        <StorageCard />
        <ArcCacheCard />
        <ServerInfoCard />
        <ServicesStatus />
      </div>
    </div>
  );
}
