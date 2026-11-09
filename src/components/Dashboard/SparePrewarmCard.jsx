import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { getStorageSpares } from "@/api/modules/system";
import { Layers, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

const STATUS_VARIANTS = {
  ready: "success",
  building: "secondary",
  consuming: "secondary",
  failed: "destructive",
  invalid: "destructive",
};

const formatAge = (seconds) => {
  if (seconds == null) return "—";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m`;
  return `${Math.max(0, Math.floor(seconds))}s`;
};

const formatBytes = (bytes) => {
  if (bytes == null) return "—";
  const gib = bytes / 1024 ** 3;
  return gib >= 1 ? `${gib.toFixed(0)} GiB` : `${Math.round(bytes / 1024 ** 2)} MiB`;
};

/**
 * Read-only view of prewarmed replacement writebacks.
 *
 * The server owns every decision here (build, expire, adopt); this card only
 * reports what is stored so an operator can tell whether prewarming is working.
 * Non-administrators receive 403 from the endpoint and see no card.
 */
const SparePrewarmCard = () => {
  const [data, setData] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(false);
  const [forbidden, setForbidden] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await getStorageSpares());
      setForbidden(false);
    } catch (err) {
      if (err?.status === 403) {
        setForbidden(true);
      } else {
        setError(err?.message || "Unable to load prewarmed replacements.");
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = setTimeout(load, 0);
    return () => clearTimeout(timer);
  }, [load]);

  if (forbidden) {
    return null;
  }

  const spares = data?.spares || [];
  const readyCount = data?.ready_count ?? 0;
  const limit = data?.max_ready_spares ?? 0;

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Layers className="size-4" />
          </span>
          Prewarmed writebacks
        </CardTitle>
        <CardDescription>
          Replacement clones built before the next boot.
        </CardDescription>
        <CardAction className="flex items-center gap-1.5">
          <Badge variant={data?.enabled ? "success" : "secondary"}>
            {data?.enabled ? "Enabled" : "Disabled"}
          </Badge>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={load}
            disabled={loading}
            title="Refresh prewarmed writebacks"
            aria-label="Refresh prewarmed writebacks"
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} />
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {error && (
          <div
            role="alert"
            className="flex flex-col items-start gap-2 rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm"
          >
            <span>Failed to load prewarmed writebacks: {error}</span>
            <Button variant="outline" size="sm" onClick={load}>
              Retry
            </Button>
          </div>
        )}

        {data && (
          <p className="text-[11px] leading-5 text-muted-foreground">
            {readyCount} of {limit} permitted clone{limit === 1 ? "" : "s"} ready · expires after{" "}
            {data.ttl_hours}h · builds pause below {formatBytes(data.min_free_bytes)} free
          </p>
        )}

        {!error && data && !data.enabled && (
          <p className="rounded-xl border border-dashed border-border/80 bg-muted/15 p-3 text-xs leading-5 text-muted-foreground">
            Prewarming is off. Enable it under Settings → Storage prewarming.
          </p>
        )}

        {!error && data?.enabled && spares.length === 0 && (
          <p className="rounded-xl border border-dashed border-border/80 bg-muted/15 p-3 text-xs leading-5 text-muted-foreground">
            No replacement clone is ready yet. One is built for each offline non-persistent
            client once its writeback has been removed.
          </p>
        )}

        {spares.length > 0 && (
          <ul aria-label="Prewarmed writebacks" className="flex flex-col gap-2">
            {spares.map((spare) => (
              <li
                key={spare.dataset}
                className="flex min-w-0 flex-col gap-1 rounded-xl border border-border/70 bg-muted/15 p-3"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-[13px] font-medium">
                    {spare.client_name || spare.client_id}
                  </p>
                  <Badge variant={STATUS_VARIANTS[spare.status] || "secondary"}>
                    {spare.status}
                  </Badge>
                  <span className="text-[10px] text-muted-foreground">
                    {formatAge(spare.age_seconds)} old
                  </span>
                  {spare.failures > 0 && (
                    <span className="text-[10px] text-muted-foreground">
                      {spare.failures} failed attempt{spare.failures === 1 ? "" : "s"}
                    </span>
                  )}
                </div>
                <p className="break-all font-mono-data text-[10px] text-muted-foreground">{spare.dataset}</p>
                <p className="break-all font-mono-data text-[10px] text-muted-foreground">
                  from {spare.source_snapshot}
                </p>
                {spare.last_error && (
                  <p className="break-words text-[11px] text-destructive">{spare.last_error}</p>
                )}
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
};

export default SparePrewarmCard;
