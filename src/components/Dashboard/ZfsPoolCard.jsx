import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useAppStore } from "@/store/useAppStore";

const ZfsPoolCard = ({ loading }) => {
  const zpoolStats = useAppStore((state) => state.zpoolStats);

  const health = zpoolStats?.health;
  const healthLabel =
    !health || health === "-" || health === "ONLINE" ? "ONLINE" : health;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Disk Usage</CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex flex-col gap-3" aria-hidden="true">
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-1/3" />
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
            <div className="flex justify-between items-center col-span-2">
              <span className="font-semibold">Health:</span>
              <Badge variant={healthLabel === "ONLINE" ? "outline" : "destructive"}>
                {healthLabel}
              </Badge>
            </div>
          </div>
        ) : (
          <div className="text-center py-4 text-destructive">
            Failed to load ZFS pool info.
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ZfsPoolCard;