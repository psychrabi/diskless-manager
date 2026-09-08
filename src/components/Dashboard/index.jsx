import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/react/shallow";
import ArcCacheCard from "./ArcCacheCard";
import ClientOverviewCard from "./ClientOverviewCard";
import MasterImageOverviewCard from "./MasterImageOverviewCard";
import MetricsCard from "./MetricsCard";
import ServerInfoCard from "./ServerInfoCard";
import ServicesStatus from "./ServicesStatus";
import StorageThroughputCard from "./StorageThroughputCard";
import ZfsPoolCard from "./ZfsPoolCard";

export default function Dashboard() {
  const { loading } = useAppStore(
    useShallow((state) => ({
      loading: state.loading,
    })),
  );

  if (loading) {
    return (
      <div className="flex flex-col gap-6">
        <Card>
          <CardHeader>
            <CardTitle>
              <Skeleton className="h-5 w-64" />
            </CardTitle>
          </CardHeader>
          <CardContent className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
            {Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="h-24 rounded-lg" />
            ))}
          </CardContent>
        </Card>
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2 xl:grid-cols-3">
          {Array.from({ length: 6 }, (_, i) => (
            <Card key={i}>
              <CardHeader>
                <Skeleton className="h-4 w-40" />
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-5/6" />
                <Skeleton className="h-4 w-4/5" />
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Server overview</h1>
        <p className="text-sm text-muted-foreground">Monitor clients, storage, and the services behind your boot environment.</p>
      </div>
      <MetricsCard />

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2 xl:grid-cols-3">
        <ServerInfoCard />
        <ServicesStatus />
        <ZfsPoolCard />
        <ArcCacheCard />
        <StorageThroughputCard />
        <ClientOverviewCard />
        <MasterImageOverviewCard />
      </div>
    </div>
  );
}
