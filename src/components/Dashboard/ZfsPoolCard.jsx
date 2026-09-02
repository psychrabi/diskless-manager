import { useAppStore } from "@/store/useAppStore";
import { Card, LoadingSkeleton, StatusBadge } from "@/components/ui";

const ZfsPoolCard = ({ loading }) => {
  const zpoolStats = useAppStore((state) => state.zpoolStats);
  return (
    <Card title="Disk Usage">
      {loading ? (
        <div className="space-y-3" aria-hidden="true">
          <LoadingSkeleton variant="text" width="3/4" />
          <LoadingSkeleton variant="text" width="1/2" />
          <LoadingSkeleton variant="text" width="2/3" />
          <LoadingSkeleton variant="text" width="3/4" />
          <LoadingSkeleton variant="text" width="1/3" />
        </div>
      ) : zpoolStats ? (
        <div className="grid grid-cols-2 gap-x-10 gap-y-2">
          <div className="flex justify-between col-span-2">
            <span className="font-semibold">Pool:</span>
            <span className="text-right">{zpoolStats.name}</span>
          </div>
          <div className="flex justify-between col-span-2">
            <span className="font-semibold">Size:</span>
            <span className="text-right">{zpoolStats.size}</span>
          </div>
          <div className="flex justify-between col-span-2">
            <span className="font-semibold">Used:</span>
            <span className="text-right">{zpoolStats.allocated}</span>
          </div>
          <div className="flex justify-between col-span-2">
            <span className="font-semibold">Available:</span>
            <span className="text-right">{zpoolStats.free}</span>
          </div>
          <div className="flex justify-between col-span-2 items-center">
            <span className="font-semibold">Health:</span>
            <StatusBadge
              status={zpoolStats.health === "-" ? "success" : zpoolStats.health === "ONLINE" ? "success" : "error"}
              size="sm"
            >
              {zpoolStats.health === "-" ? "Good" : zpoolStats.health}
            </StatusBadge>
          </div>
        </div>
      ) : (
        <div className="text-error text-center py-4">Failed to load ZFS pool info.</div>
      )}
    </Card>
  );
};

export default ZfsPoolCard;
