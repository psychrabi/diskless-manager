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
    <Card className="h-full xl:col-span-3">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Activity className="size-4 text-muted-foreground" />
          Storage maintenance
        </CardTitle>
        <CardAction className="flex flex-wrap gap-1">
          <Button
            variant="outline"
            size="sm"
            onClick={() => load(true)}
            disabled={Boolean(busy)}
          >
            <RefreshCw data-icon="inline-start" />
            Refresh
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
      <CardContent className="flex flex-col gap-3">
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : !status ? (
          <p className="py-2 text-sm text-muted-foreground">Loading storage health…</p>
        ) : (
          <>
            {pool ? (
              <div className="divide-y divide-border">
                <div className="flex items-center justify-between gap-4 py-2">
                  <span className="text-sm text-muted-foreground">Pool</span>
                  <span className="text-sm font-medium break-all">{pool.pool}</span>
                </div>
                <div className="flex items-center justify-between gap-4 py-2">
                  <span className="text-sm text-muted-foreground">Health</span>
                  <Badge variant={pool.state === "ONLINE" ? "outline" : "destructive"}>
                    {pool.state}
                  </Badge>
                </div>
                <div className="flex flex-col gap-1 py-2">
                  <span className="text-sm text-muted-foreground">Last / active scan</span>
                  <span className="text-xs break-words">
                    {pool.scan || "No scan has been recorded on this pool."}
                  </span>
                  {pool.scan_in_progress && (
                    <span className="text-xs text-muted-foreground">
                      {pool.scan_kind ?? "scan"} {formatPercent(pool.scan_percent)} complete
                    </span>
                  )}
                </div>
                {pool.vdev_errors?.some(
                  (vdev) => vdev.read || vdev.write || vdev.checksum
                ) && (
                  <div className="py-2 text-xs text-destructive" data-testid="vdev-errors">
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
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">No ZFS pool was found.</p>
            )}

            <div className="border-t pt-2">
              <p className="text-sm text-muted-foreground">Devices (SMART)</p>
              {status.smart?.length ? (
                <ul className="mt-1 flex flex-col gap-1 text-xs">
                  {status.smart.slice(0, 8).map((device) => (
                    <li key={device.device} className="flex flex-wrap items-center gap-2">
                      <span className="font-medium">{device.device}</span>
                      {device.available ? (
                        <>
                          <Badge variant={device.health === "passed" ? "outline" : "destructive"}>
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
                <p className="mt-1 text-xs text-muted-foreground">
                  No physical pool members were reported.
                </p>
              )}
            </div>

            <div className="border-t pt-2">
              <p className="text-sm text-muted-foreground">History</p>
              {status.runs?.length ? (
                <ul className="mt-1 flex flex-col gap-1 text-xs">
                  {status.runs.slice(0, 5).map((run) => (
                    <li key={run.id} className="flex flex-wrap items-center gap-2">
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
                <p className="mt-1 text-xs text-muted-foreground">No maintenance runs yet.</p>
              )}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
