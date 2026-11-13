import { useCallback, useEffect, useState } from "react";
import { Activity, RefreshCw } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import {
  getMaintenanceStatus,
  startMaintenance,
  stopMaintenance,
} from "@/api/modules/system";
import { useToastStore } from "@/store/useToastStore";

const formatPercent = (value) =>
  value == null ? "—" : `${Number(value).toFixed(1)}%`;

const stateVariant = (state) => {
  if (state === "completed") return "outline";
  if (state === "failed") return "destructive";
  return "secondary";
};

const MicroLabel = ({ children }) => (
  <p className="text-[11px] font-medium text-muted-foreground">
    {children}
  </p>
);

/**
 * Read-mostly storage health: pool state, the active scan, SMART health and the
 * recorded history. Starting or stopping a scan is the only mutation and the
 * server refuses to collide with an already-active scan.
 */
export default function StorageMaintenanceCard() {
  const { success, error: showError } = useToastStore();
  const [status, setStatus] = useState(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");

  const load = useCallback(
    async (refresh = false) => {
      try {
        const next = await getMaintenanceStatus(refresh);
        setStatus(next);
        setError("");
      } catch (cause) {
        setError(cause?.message || String(cause));
      }
    },
    []
  );

  // Initial load and polling live in one effect with a cancellation guard so
  // an unmounted card cannot write state.
  useEffect(() => {
    let active = true;
    const tick = async (refresh = false) => {
      try {
        const next = await getMaintenanceStatus(refresh);
        if (!active) return;
        setStatus(next);
        setError("");
      } catch (cause) {
        if (!active) return;
        setError(cause?.message || String(cause));
      }
    };
    void tick();
    const id = setInterval(() => void tick(), 60000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  const start = async (operation) => {
    setBusy(operation);
    try {
      await startMaintenance(operation);
      success("Storage maintenance", `Started ${operation}.`);
      await load(true);
    } catch (cause) {
      showError("Storage maintenance", cause?.message || String(cause));
    } finally {
      setBusy("");
    }
  };

  const stop = async () => {
    setBusy("stop");
    try {
      await stopMaintenance();
      success("Storage maintenance", "Stopped the active scan.");
      await load(true);
    } catch (cause) {
      showError("Storage maintenance", cause?.message || String(cause));
    } finally {
      setBusy("");
    }
  };

  const pool = status?.pool;
  const scanActive = Boolean(pool?.scan_in_progress);

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Activity className="size-4" />
          </span>
          Storage maintenance
        </CardTitle>
        <CardAction className="flex flex-wrap gap-1.5">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => load(true)}
            disabled={Boolean(busy)}
            title="Refresh storage health"
            aria-label="Refresh storage health"
          >
            <RefreshCw className={busy ? "animate-spin" : undefined} />
          </Button>
          {scanActive ? (
            <Button variant="destructive" size="sm" onClick={stop} disabled={Boolean(busy)}>
              {busy === "stop" ? <Spinner data-icon="inline-start" /> : null}
              Stop scan
            </Button>
          ) : (
            <>
              <Button
                variant="outline"
                size="sm"
                onClick={() => start("scrub")}
                disabled={Boolean(busy) || !pool}
              >
                {busy === "scrub" ? <Spinner data-icon="inline-start" /> : null}
                Scrub
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => start("trim")}
                disabled={Boolean(busy) || !pool}
              >
                {busy === "trim" ? <Spinner data-icon="inline-start" /> : null}
                TRIM
              </Button>
            </>
          )}
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error ? (
          <p role="alert" className="rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
            {error}
          </p>
        ) : !status ? (
          <p className="py-2 text-sm text-muted-foreground">Loading storage health…</p>
        ) : (
          <>
            {pool ? (
              <>
                <div className="grid gap-3 sm:grid-cols-2">
                  <div>
                    <MicroLabel>Pool</MicroLabel>
                    <p className="mt-1.5 truncate font-mono-data text-[13px] font-medium">{pool.pool}</p>
                  </div>
                  <div>
                    <MicroLabel>Health</MicroLabel>
                    <div className="mt-1.5">
                      <Badge variant={pool.state === "ONLINE" ? "success" : "destructive"}>
                        {pool.state}
                      </Badge>
                    </div>
                  </div>
                </div>

                <div className="border-t border-border/60 pt-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <MicroLabel>Last / active scan</MicroLabel>
                    {scanActive && <Badge variant="info">In progress</Badge>}
                  </div>
                  <p className="mt-1.5 text-[13px] leading-5 text-foreground">
                    {pool.scan || "No scan has been recorded on this pool."}
                  </p>
                  {pool.scan_in_progress && (
                    <>
                      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
                        <div
                          className="h-full rounded-full bg-primary"
                          style={{ width: `${Math.max(0, Math.min(100, Number(pool.scan_percent) || 0))}%` }}
                        />
                      </div>
                      <p className="mt-1.5 text-[11px] text-muted-foreground">
                        {pool.scan_kind ?? "scan"} {formatPercent(pool.scan_percent)} complete
                      </p>
                    </>
                  )}
                </div>

                {pool.vdev_errors?.some(
                  (vdev) => vdev.read || vdev.write || vdev.checksum
                ) && (
                  <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive" data-testid="vdev-errors">
                    {pool.vdev_errors
                      .filter((vdev) => vdev.read || vdev.write || vdev.checksum)
                      .map((vdev) => (
                        <p key={vdev.name}>
                          {vdev.name}: read {vdev.read} · write {vdev.write} · cksum{" "}
                          {vdev.checksum}
                        </p>
                      ))}
                  </div>
                )}
              </>
            ) : (
              <p className="text-sm text-muted-foreground">No ZFS pool was found.</p>
            )}

            <div className="grid gap-4 border-t border-border/60 pt-4 sm:grid-cols-2">
              <div className="min-w-0">
                <MicroLabel>Devices (SMART)</MicroLabel>
                {status.smart?.length ? (
                  <ul className="mt-2 flex flex-col gap-2 text-xs">
                    {status.smart.slice(0, 8).map((device) => (
                      <li key={device.device} className="flex flex-wrap items-center gap-1.5">
                        <span className="min-w-0 break-all font-mono-data text-[10px] text-foreground">
                          {device.device}
                        </span>
                        {device.available ? (
                          <>
                            <Badge variant={device.health === "passed" ? "success" : "destructive"}>
                              {device.health}
                            </Badge>
                            {device.temperature_c != null && (
                              <span className="text-muted-foreground">
                                {device.temperature_c}°C
                              </span>
                            )}
                            {device.wear_percent != null && (
                              <span className="text-muted-foreground">
                                wear {device.wear_percent}%
                              </span>
                            )}
                            {device.media_errors != null && device.media_errors > 0 && (
                              <span className="text-destructive">
                                media errors {device.media_errors}
                              </span>
                            )}
                          </>
                        ) : (
                          <span className="text-muted-foreground">
                            {device.note || "unavailable"}
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="mt-2 text-xs text-muted-foreground">
                    No physical pool members were reported.
                  </p>
                )}
              </div>

              <div className="min-w-0">
                <MicroLabel>History</MicroLabel>
                {status.runs?.length ? (
                  <ul className="mt-2 flex flex-col gap-2 text-xs">
                    {status.runs.slice(0, 5).map((run) => (
                      <li key={run.id} className="flex flex-wrap items-center gap-1.5">
                        <Badge variant={stateVariant(run.state)}>{run.state}</Badge>
                        <span className="font-medium">{run.operation}</span>
                        <span className="text-muted-foreground">
                          {run.finished_at || run.started_at}
                        </span>
                        {run.actor && (
                          <span className="text-muted-foreground">· {run.actor}</span>
                        )}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="mt-2 text-xs text-muted-foreground">No maintenance runs yet.</p>
                )}
              </div>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
