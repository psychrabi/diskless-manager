import { Database } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { useAppStore } from "@/store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";

const formatRate = (value) => (value == null ? "—" : `${value.toFixed(2)} MB/s`);

const StorageCard = () => {
  const zpoolStats = useAppStore((state) => state.zpoolStats);
  const { metrics, error } = useMetrics();
  const storage = metrics?.storage;
  const zfs = storage?.zfs;

  const health = zpoolStats?.health;
  const healthLabel =
    !health || health === "-" || health === "ONLINE" ? "ONLINE" : health;

  return (
    <Card className="h-full xl:col-span-2">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Database className="size-4 text-muted-foreground" />
          Storage
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {zpoolStats ? (
          <div className="divide-y divide-border">
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Pool</span>
              <span className="text-sm font-medium text-right break-all">{zpoolStats.name}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Size</span>
              <span className="text-sm font-medium tabular-nums text-right">{zpoolStats.size}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Used</span>
              <span className="text-sm font-medium tabular-nums text-right">{zpoolStats.allocated}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Available</span>
              <span className="text-sm font-medium tabular-nums text-right">{zpoolStats.free}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Health</span>
              <Badge variant={healthLabel === "ONLINE" ? "outline" : "destructive"}>
                {healthLabel}
              </Badge>
            </div>
          </div>
        ) : (
          <div className="py-2 text-center text-sm text-destructive">
            Failed to load ZFS pool info.
          </div>
        )}

        <Separator />

        {error ? (
          <p className="py-2 text-center text-sm text-destructive">Metrics stream is unavailable.</p>
        ) : storage?.warming_up ? (
          <p className="py-2 text-center text-sm text-muted-foreground">
            Collecting a second sample…
          </p>
        ) : zfs ? (
          <div className="divide-y divide-border">
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Read</span>
              <span className="text-sm font-medium tabular-nums text-right">{formatRate(zfs.read_speed_mbps)}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Write</span>
              <span className="text-sm font-medium tabular-nums text-right">{formatRate(zfs.write_speed_mbps)}</span>
            </div>
            <div className="flex items-center justify-between gap-4 py-2">
              <span className="text-sm text-muted-foreground">Total</span>
              <span className="text-sm font-medium tabular-nums text-right">{formatRate((zfs.read_speed_mbps ?? 0) + (zfs.write_speed_mbps ?? 0))}</span>
            </div>
          </div>
        ) : (
          <p className="py-2 text-center text-sm text-muted-foreground">
            ZFS kstat counters are unavailable.
          </p>
        )}
      </CardContent>
    </Card>
  );
};

export default StorageCard;
