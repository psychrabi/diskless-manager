import { HardDrive } from "lucide-react";
import { useMetrics } from "@/contexts/useMetrics";
import { Card } from "@/components/ui";

const formatRate = (value) => (value == null ? "—" : `${value.toFixed(2)} MB/s`);

const StorageThroughputCard = () => {
  const { metrics, error } = useMetrics();
  const storage = metrics?.storage;
  const zfs = storage?.zfs;

  return (
    <Card title="ZFS Throughput" icon={HardDrive} subtitle="Measured from ZFS pool kstats">
      {error ? (
        <p className="text-error text-sm">Metrics stream is unavailable.</p>
      ) : storage?.warming_up ? (
        <p className="text-sm text-base-content/60">Collecting a second sample…</p>
      ) : zfs ? (
        <div className="space-y-2 font-mono text-sm">
          <div className="flex justify-between"><span>Read</span><span>{formatRate(zfs.read_speed_mbps)}</span></div>
          <div className="flex justify-between"><span>Write</span><span>{formatRate(zfs.write_speed_mbps)}</span></div>
        </div>
      ) : (
        <p className="text-sm text-base-content/60">ZFS kstat counters are unavailable.</p>
      )}
    </Card>
  );
};

export default StorageThroughputCard;
