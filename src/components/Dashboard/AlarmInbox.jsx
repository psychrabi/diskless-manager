import { acknowledgeAlarm, listAlarms } from "@/api/modules/alarms";
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
import {
  AlertTriangle,
  Bell,
  CheckCircle2,
  RefreshCw,
  ShieldAlert,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

const KIND_LABELS = {
  boot_failure: "Boot failure",
  storage_recovery: "Storage recovery",
  service: "Service",
  pool: "Storage pool",
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
        className="size-4 shrink-0 text-amber-600 dark:text-amber-400"
        aria-hidden="true"
      />
    );
  }
  return <Bell className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />;
};

const AlarmRow = ({ alarm, busy, onAcknowledge }) => {
  const observed = formatTime(alarm.observed_at);
  const acknowledged = Boolean(alarm.acknowledged_at);

  return (
    <li
      className={`flex flex-col gap-3 rounded-lg border p-3 sm:flex-row sm:items-start ${
        acknowledged ? "border-border/60 bg-muted/20" : "border-border"
      }`}
    >
      <SeverityIcon severity={alarm.severity} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-medium">{alarm.title}</p>
          <Badge variant={alarm.severity === "critical" ? "destructive" : "outline"}>
            {alarm.severity}
          </Badge>
          <Badge variant="secondary">{KIND_LABELS[alarm.kind] || alarm.kind}</Badge>
          {acknowledged && (
            <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
              <CheckCircle2 className="size-3" aria-hidden="true" />
              Acknowledged
              {alarm.acknowledged_by ? ` by ${alarm.acknowledged_by}` : ""}
            </span>
          )}
        </div>
        <p className="mt-1 break-words text-sm text-muted-foreground">
          {alarm.detail}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {alarm.source}
          {observed ? ` · ${observed}` : ""}
        </p>
      </div>
      {!acknowledged && (
        <Button
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() => onAcknowledge(alarm.id)}
        >
          {busy ? "Saving…" : "Acknowledge"}
        </Button>
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
    <Card>
      <CardHeader>
        <CardTitle>Alarms</CardTitle>
        <CardDescription>
          Boot failures, storage recovery, service outages, and pool health.
        </CardDescription>
        <CardAction>
          <Button
            variant="ghost"
            size="icon"
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
            className="mb-3 flex items-center justify-between gap-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm"
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
            className="mb-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm"
          >
            {actionError}
          </div>
        )}

        {activeCount === 0 && alarms.length === 0 ? (
          <div className="flex items-center gap-3 rounded-lg border border-border bg-muted/20 p-4 text-sm text-muted-foreground">
            <CheckCircle2 className="size-4 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />
            No active alarms.
          </div>
        ) : (
          <>
            <p className="mb-3 text-xs text-muted-foreground" aria-live="polite">
              {summary?.critical || 0} critical, {summary?.warning || 0} warning
              {summary?.acknowledged
                ? `, ${summary.acknowledged} acknowledged`
                : ""}
            </p>
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
