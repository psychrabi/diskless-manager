import { Database, MoveDown, MoveUp } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useAppStore } from "@/store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";

const formatRate = (value) => (value == null ? "—" : `${value.toFixed(2)} MB/s`);

const parseSize = (value) => {
  if (typeof value === "number") return value;
  const match = /^([\d.]+)\s*([KMGTPE]?)(i?B)?$/i.exec(String(value ?? "").trim());
  if (!match) return null;
  const units = { K: 1024, M: 1024 ** 2, G: 1024 ** 3, T: 1024 ** 4, P: 1024 ** 5, E: 1024 ** 6 };
  return Number(match[1]) * (units[match[2].toUpperCase()] || 1);
};

const StorageCard = () => {
  const zpoolStats = useAppStore((state) => state.zpoolStats);
  const { metrics, error } = useMetrics();
  const storage = metrics?.storage;
  const zfs = storage?.zfs;

  const health = zpoolStats?.health;
  const healthLabel =
    !health || health === "-" || health === "ONLINE" ? "ONLINE" : health;
  const used = parseSize(zpoolStats?.allocated);
  const size = parseSize(zpoolStats?.size);
  const usedPct = used != null && size ? Math.min(100, Math.round((used / size) * 100)) : null;

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Database className="size-4" />
          </span>
          Storage
        </CardTitle>
        {zpoolStats && (
          <CardAction>
            <Badge variant={healthLabel === "ONLINE" ? "success" : "destructive"} className="gap-1.5">
              <span className="size-1.5 rounded-full bg-current" />
              {healthLabel}
            </Badge>
          </CardAction>
        )}
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {zpoolStats ? (
          <>
            <div className="flex items-end justify-between gap-4">
              <div className="min-w-0">
                <p className="text-xs font-medium text-muted-foreground">
                  Allocated
                </p>
                <p className="mt-1 text-3xl font-semibold tracking-[-0.045em] tabular-nums text-foreground">
                  {zpoolStats.allocated}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">of {zpoolStats.size}</p>
              </div>
              <div className="min-w-0 text-right">
                <p className="text-xs font-medium text-muted-foreground">
                  Pool
                </p>
                <p className="mt-1 truncate font-mono-data text-sm font-medium text-foreground">
                  {zpoolStats.name}
                </p>
              </div>
            </div>

            <div>
              <div className="h-2 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-gradient-to-r from-primary to-primary/70 transition-[width] duration-500"
                  style={{ width: `${usedPct ?? 0}%` }}
                />
              </div>
              <div className="mt-2 flex items-center justify-between text-[11px] text-muted-foreground">
                <span>{usedPct == null ? "Usage unavailable" : `${usedPct}% of pool used`}</span>
                <span>{zpoolStats.free} free</span>
              </div>
            </div>

            <div className="grid grid-cols-3 gap-2 border-t border-border/60 pt-4">
              {[
                { icon: MoveUp, label: "Read", value: formatRate(zfs?.read_speed_mbps) },
                { icon: MoveDown, label: "Write", value: formatRate(zfs?.write_speed_mbps) },
                { icon: Database, label: "Total", value: formatRate((zfs?.read_speed_mbps ?? 0) + (zfs?.write_speed_mbps ?? 0)) },
              ].map(({ icon: Icon, label, value }) => (
                <div key={label} className="min-w-0">
                  <div className="flex items-center gap-1 text-[11px] font-medium text-muted-foreground">
                    <Icon className="size-3" aria-hidden="true" />
                    {label}
                  </div>
                  <p className="mt-1 truncate font-mono-data text-[11px] font-medium text-foreground">
                    {value}
                  </p>
                </div>
              ))}
            </div>
          </>
        ) : (
          <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
            Failed to load ZFS pool info.
          </div>
        )}

        {error ? (
          <p className="text-center text-sm text-destructive">Metrics stream is unavailable.</p>
        ) : storage?.warming_up ? (
          <p className="text-center text-xs text-muted-foreground">Collecting a second sample…</p>
        ) : !zfs ? (
          <p className="text-center text-xs text-muted-foreground">ZFS kstat counters are unavailable.</p>
        ) : null}
      </CardContent>
    </Card>
  );
};

export default StorageCard;
