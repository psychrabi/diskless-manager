import { useEffect, useRef, useState } from "react";
import { Gauge, RefreshCw } from "lucide-react";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { clearRamCache } from "@/api/modules/system";

const formatBytes = (value) => {
  if (value == null) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let v = Number(value);
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
};

const formatPercent = (value) =>
  value == null ? "—" : `${value.toFixed(1)}%`;

const StatRow = ({ label, value, hint }) => (
  <div className="flex items-center justify-between gap-4 py-2">
    <span className="text-sm text-muted-foreground">{label}</span>
    <span className="text-sm font-medium tabular-nums text-right">
      {value}
      {hint && (
        <span className="text-xs ml-1 font-normal text-muted-foreground">{hint}</span>
      )}
    </span>
  </div>
);

const ArcCacheCard = () => {
  const arcStat = useAppStore((state) => state.arcStat);
  const fetchArcStat = useAppStore((state) => state.fetchArcStat);
  const fetchArcStatRef = useRef(fetchArcStat);
  const { success, error: showError } = useToastStore();
  const [isClearing, setIsClearing] = useState(false);

  useEffect(() => {
    fetchArcStatRef.current = fetchArcStat;
  }, [fetchArcStat]);

  // Keep the (cumulative) ARC counters reasonably fresh.
  useEffect(() => {
    const id = setInterval(() => {
      fetchArcStatRef.current?.();
    }, 15000);
    return () => clearInterval(id);
  }, []);

  const handleClearCache = async () => {
    setIsClearing(true);
    try {
      const response = await clearRamCache();
      success("Cache Cleared", response?.message || "Cache cleared successfully");
      fetchArcStatRef.current?.();
    } catch (err) {
      showError(
        "Clear Cache",
        `Failed to clear cache: ${err.message || String(err)}`
      );
    } finally {
      setIsClearing(false);
    }
  };

  return (
    <Card className="h-full xl:col-span-2">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Gauge className="size-4 text-muted-foreground" />
          ZFS ARC Cache
        </CardTitle>
        <CardAction>
          <Button
            onClick={handleClearCache}
            variant="outline"
            size="sm"
            disabled={isClearing}
            title="Clear system cache (sync + drop_caches)"
          >
            {isClearing ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <RefreshCw data-icon="inline-start" />
            )}
            Clear Cache
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {arcStat ? (
          <div>
            <div className="divide-y divide-border">
              <StatRow
                label="Cache Size"
                value={formatBytes(arcStat.size)}
                hint={`/ ${formatBytes(arcStat.max_size)}`}
              />
              <StatRow
                label="Cache Usage"
                value={formatPercent(arcStat.used_percent)}
              />
              <StatRow label="Hit Rate" value={formatPercent(arcStat.hit_ratio)} />
              <StatRow
                label="Hit Rate (recent)"
                value={
                  arcStat.interval_warming_up
                    ? "Warming up..."
                    : formatPercent(arcStat.interval_hit_ratio)
                }
                hint={arcStat.interval_warming_up ? null : "last ~15s"}
              />
              <StatRow
                label="Demand Data"
                value={formatPercent(arcStat.demand_data_hit_ratio)}
              />
              <StatRow
                label="Hits / Misses"
                value={`${arcStat.hits ?? 0} / ${arcStat.misses ?? 0}`}
              />
              <StatRow
                label="L2ARC"
                value={
                  arcStat.l2_size > 0
                    ? formatBytes(arcStat.l2_size)
                    : "Not configured"
                }
                hint={
                  arcStat.l2_size > 0 ? formatPercent(arcStat.l2_hit_ratio) : null
                }
              />
            </div>
            {!arcStat.interval_warming_up && arcStat.interval_hit_ratio > 0 && (
              <div className="pt-3">
                <Progress value={arcStat.interval_hit_ratio} />
              </div>
            )}
          </div>
        ) : (
          <div className="py-4 text-center text-sm text-destructive">
            ARC stats unavailable. Is ZFS /proc kstat present?
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ArcCacheCard;