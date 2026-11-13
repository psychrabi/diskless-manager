import { acknowledgeAlarm, listAlarms } from "@/api/modules/alarms";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  AlertTriangle,
  Bell,
  CheckCircle2,
  ExternalLink,
  RefreshCw,
  ShieldAlert,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";

const KIND_LABELS = {
  boot_failure: "Boot failure",
  storage_recovery: "Storage recovery",
  service: "Service",
  pool: "Storage pool",
  spare_prewarm: "Prewarmed writeback",
  maintenance: "Storage maintenance",
};

const formatTime = (value) => {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
};

const SeverityIcon = ({ severity }) => {
  if (severity === "critical") {
    return <ShieldAlert className="size-4 shrink-0 text-destructive" aria-hidden="true" />;
  }
  if (severity === "warning") {
    return (
      <AlertTriangle
        className="size-4 shrink-0 text-warning"
        aria-hidden="true"
      />
    );
  }
  return <Bell className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />;
};

const AlarmRow = ({ alarm, busy, onAcknowledge }) => {
  const observed = formatTime(alarm.observed_at);
  const acknowledged = Boolean(alarm.acknowledged_at);
  const critical = alarm.severity === "critical";
  const hasActions =
    !acknowledged ||
    Boolean(alarm.client_id) ||
    alarm.kind === "storage_recovery";

  return (
    <li
      className={`flex flex-col gap-3 rounded-xl border p-4 ${
        acknowledged
          ? "border-border/60 bg-muted/20"
          : critical
            ? "border-destructive/20 bg-destructive/[0.035]"
            : "border-warning/20 bg-warning/[0.035]"
      }`}
    >
      <div className="flex items-start gap-3">
        <span
          className={`flex size-8 shrink-0 items-center justify-center rounded-lg border ${
            critical
              ? "border-destructive/15 bg-destructive/10"
              : "border-warning/20 bg-warning/10"
          }`}
        >
          <SeverityIcon severity={alarm.severity} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="font-medium text-foreground">{alarm.title}</p>
            <Badge variant={critical ? "destructive" : "warning"}>{alarm.severity}</Badge>
            <Badge variant="secondary">{KIND_LABELS[alarm.kind] || alarm.kind}</Badge>
            {acknowledged && (
              <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
                <CheckCircle2 className="size-3" aria-hidden="true" />
                Dismissed
                {alarm.acknowledged_by ? ` by ${alarm.acknowledged_by}` : ""}
              </span>
            )}
          </div>
          <p className="mt-1.5 break-words text-[13px] leading-5 text-muted-foreground">
            {alarm.detail}
          </p>
          <p className="mt-1 font-mono-data text-[10px] tracking-[0.04em] text-muted-foreground/70">
            {alarm.source}
            {observed ? ` · ${observed}` : ""}
          </p>
        </div>
      </div>
      {/* Actions sit on their own line so the title and detail keep the full
          card width however many links an alarm carries. */}
      {hasActions && (
        <div className="flex flex-wrap items-center gap-2 pl-11">
          {alarm.client_id && (
            <Link
              to={`/clients?client=${encodeURIComponent(alarm.client_id)}`}
              className={buttonVariants({ variant: "ghost", size: "sm" })}
              title={`Open ${alarm.source}`}
            >
              <ExternalLink data-icon="inline-start" />
              Open client
            </Link>
          )}
          {alarm.kind === "storage_recovery" && (
            <Link
              to="/pending-recovery"
              className={buttonVariants({ variant: "ghost", size: "sm" })}
              title="Open pending recovery"
            >
              <ExternalLink data-icon="inline-start" />
              View recovery
            </Link>
          )}
          {!acknowledged && (
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => onAcknowledge(alarm.id)}
              title="Hide this alarm. The underlying fault stays active until it is fixed."
            >
              {busy ? "Saving…" : "Dismiss"}
            </Button>
          )}
        </div>
      )}
    </li>
  );
};

const AlarmInbox = () => {
  const [summary, setSummary] = useState(null);
  const [error, setError] = useState(null);
  const [actionError, setActionError] = useState(null);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setSummary(await listAlarms());
    } catch (err) {
      setError(err?.message || "Unable to load alarms.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = setTimeout(load, 0);
    return () => clearTimeout(timer);
  }, [load]);

  const handleAcknowledge = useCallback(
    async (id) => {
      setBusyId(id);
      setActionError(null);
      try {
        await acknowledgeAlarm(id);
        await load();
      } catch (err) {
        setActionError(err?.message || "Unable to acknowledge the alarm.");
      } finally {
        setBusyId(null);
      }
    },
    [load],
  );

  const alarms = summary?.alarms || [];
  const activeCount = (summary?.critical || 0) + (summary?.warning || 0);

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border/70 bg-muted/50 text-muted-foreground">
            <Bell className="size-4" />
          </span>
          Alarms
          {activeCount > 0 && <Badge variant="destructive">{activeCount}</Badge>}
        </CardTitle>
        <CardDescription>
          Boot failures, storage recovery, service outages, pool health, and prewarmed
          writebacks. Dismissing an alarm hides it — it does not fix the fault.
        </CardDescription>
        <CardAction>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={load}
            disabled={loading}
            title="Refresh alarms"
            aria-label="Refresh alarms"
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} />
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {error && (
          <div
            role="alert"
            className="mb-3 flex items-center justify-between gap-3 rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm"
          >
            <span>Failed to load alarms: {error}</span>
            <Button variant="outline" size="sm" onClick={load}>
              Retry
            </Button>
          </div>
        )}
        {actionError && (
          <div
            role="alert"
            className="mb-3 rounded-xl border border-destructive/30 bg-destructive/5 p-3 text-sm"
          >
            {actionError}
          </div>
        )}

        {activeCount === 0 && alarms.length === 0 ? (
          <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border/80 bg-muted/15 px-6 py-10 text-center">
            <span className="flex size-10 items-center justify-center rounded-full border border-success/20 bg-success/10">
              <CheckCircle2 className="size-5 text-success" aria-hidden="true" />
            </span>
            <p className="mt-3 text-sm font-medium text-foreground">No active alarms.</p>
            <p className="mt-1 text-xs text-muted-foreground">Every watched subsystem is quiet.</p>
          </div>
        ) : (
          <>
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <Badge variant="destructive">{summary?.critical || 0} critical</Badge>
              <Badge variant="warning">{summary?.warning || 0} warning</Badge>
              {summary?.acknowledged ? (
                <Badge variant="outline">{summary.acknowledged} dismissed</Badge>
              ) : null}
            </div>
            <ul aria-label="Operational alarms" className="flex flex-col gap-2">
              {alarms.map((alarm) => (
                <AlarmRow
                  key={alarm.id}
                  alarm={alarm}
                  busy={busyId === alarm.id}
                  onAcknowledge={handleAcknowledge}
                />
              ))}
            </ul>
          </>
        )}
      </CardContent>
    </Card>
  );
};

export default AlarmInbox;
