import { HardDrive } from "lucide-react";
import { useMetrics } from "@/contexts/useMetrics";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

const formatRate = (value) => (value == null ? "—" : `${value.toFixed(2)} MB/s`);

const StorageThroughputCard = () => {
  const { metrics, error } = useMetrics();
  const storage = metrics?.storage;
  const zfs = storage?.zfs;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <HardDrive className="size-4" />
          ZFS Throughput
        </CardTitle>
      </CardHeader>
      <CardContent>
        {error ? (
          <p className="text-sm text-destructive">Metrics stream is unavailable.</p>
        ) : storage?.warming_up ? (
          <p className="text-sm text-muted-foreground">
            Collecting a second sample…
          </p>
        ) : zfs ? (
          <div className="flex flex-col gap-2 font-mono text-sm">
            <div className="flex justify-between">
              <span>Read</span>
              <span>{formatRate(zfs.read_speed_mbps)}</span>
            </div>
            <div className="flex justify-between">
              <span>Write</span>
              <span>{formatRate(zfs.write_speed_mbps)}</span>
            </div>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            ZFS kstat counters are unavailable.
          </p>
        )}
      </CardContent>
    </Card>
  );
};

export default StorageThroughputCard;