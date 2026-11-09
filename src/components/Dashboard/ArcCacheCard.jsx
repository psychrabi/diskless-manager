import { useEffect, useRef, useState } from "react";
import { Gauge, RefreshCw } from "lucide-react";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
  value == null ? "—" : `${Number(value).toFixed(1)}%`;

const StatRow = ({ label, value, hint }) => (
  <div className="flex items-baseline justify-between gap-3 border-b border-border/50 py-2 last:border-0">
    <span className="text-[11px] text-muted-foreground">{label}</span>
    <span className="text-right font-mono-data text-[11px] font-medium tabular-nums text-foreground">
      {value}
      {hint && (
        <span className="ml-1 font-sans text-[10px] font-normal text-muted-foreground">{hint}</span>
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

  const recentHitRate = arcStat?.interval_warming_up ? null : arcStat?.interval_hit_ratio;

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Gauge className="size-4" />
          </span>
          ZFS ARC Cache
        </CardTitle>
        <CardAction>
          <Button
            onClick={handleClearCache}
            variant="ghost"
            size="icon-sm"
            disabled={isClearing}
            title="Clear system cache (sync + drop_caches)"
            aria-label="Clear system cache"
          >
            {isClearing ? <Spinner /> : <RefreshCw />}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {arcStat ? (
          <div>
            <div className="flex items-end justify-between gap-4">
              <div>
                <p className="text-xs font-medium text-muted-foreground">
                  Recent hit rate
                </p>
                <p className="mt-1.5 text-3xl font-semibold tracking-[-0.045em] tabular-nums text-foreground">
                  {recentHitRate == null ? "—" : formatPercent(recentHitRate)}
                </p>
                <p className="mt-1 text-[11px] text-muted-foreground">
                  {arcStat.interval_warming_up ? "Warming up…" : "last ~15s"}
                </p>
              </div>
              <div className="mb-1 flex-1">
                <div className="ml-auto h-1.5 w-full max-w-36 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary"
                    style={{ width: `${Math.max(0, Math.min(100, Number(recentHitRate) || 0))}%` }}
                  />
                </div>
              </div>
            </div>

            <div className="mt-4 grid grid-cols-1 gap-x-5 sm:grid-cols-2">
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
                value={formatPercent(arcStat.interval_hit_ratio)}
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
          </div>
        ) : (
          <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
            ARC stats unavailable. Is ZFS /proc kstat present?
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ArcCacheCard;
