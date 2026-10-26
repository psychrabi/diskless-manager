import { getAuditEvents } from "@/api/modules/control";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { RefreshCw, Search } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

const DEFAULT_FILTERS = {
  actor: "",
  module: "",
  status: "",
  start_date: "",
  end_date: "",
};

const STATUS_BADGE_VARIANT = {
  success: "default",
  failed: "destructive",
};

const formatTimestamp = (timestamp) => {
  try {
    return new Date(timestamp).toLocaleString();
  } catch {
    return timestamp;
  }
};

const SystemAuditLogView = () => {
  const [events, setEvents] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [filters, setFilters] = useState(DEFAULT_FILTERS);

  const fetchEvents = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await getAuditEvents(filters);
      setEvents(response.events || []);
    } catch (err) {
      setError(err?.message || String(err));
      setEvents([]);
    } finally {
      setLoading(false);
    }
  }, [filters]);

  useEffect(() => {
    // Defer so state updates are not synchronous inside the effect body.
    const timer = setTimeout(fetchEvents, 0);
    return () => clearTimeout(timer);
  }, [fetchEvents]);

  const updateFilter = (field, value) =>
    setFilters((previous) => ({ ...previous, [field]: value }));

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-lg border border-border bg-muted/30 p-4">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
          <Field>
            <Label htmlFor="audit-event-actor" className="text-sm">
              Actor
            </Label>
            <Input
              id="audit-event-actor"
              placeholder="Any administrator"
              value={filters.actor}
              onChange={(event) => updateFilter("actor", event.target.value)}
            />
          </Field>
          <Field>
            <Label htmlFor="audit-event-module" className="text-sm">
              Module
            </Label>
            <Input
              id="audit-event-module"
              placeholder="Any module"
              value={filters.module}
              onChange={(event) => updateFilter("module", event.target.value)}
            />
          </Field>
          <Field>
            <Label htmlFor="audit-event-status" className="text-sm">
              Status
            </Label>
            <NativeSelect
              id="audit-event-status"
              value={filters.status}
              onChange={(event) => updateFilter("status", event.target.value)}
            >
              <NativeSelectOption value="">Any status</NativeSelectOption>
              <NativeSelectOption value="success">Success</NativeSelectOption>
              <NativeSelectOption value="failed">Failed</NativeSelectOption>
            </NativeSelect>
          </Field>
          <Field>
            <Label htmlFor="audit-event-start" className="text-sm">
              Start Date
            </Label>
            <Input
              id="audit-event-start"
              type="date"
              value={filters.start_date}
              onChange={(event) => updateFilter("start_date", event.target.value)}
            />
          </Field>
          <Field>
            <Label htmlFor="audit-event-end" className="text-sm">
              End Date
            </Label>
            <Input
              id="audit-event-end"
              type="date"
              value={filters.end_date}
              onChange={(event) => updateFilter("end_date", event.target.value)}
            />
          </Field>
        </div>
        <div className="mt-3 flex justify-end">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setFilters(DEFAULT_FILTERS)}
          >
            Clear Filters
          </Button>
        </div>
      </div>

      {error ? (
        <div
          role="alert"
          className="flex items-center justify-between gap-4 rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm"
        >
          <span>Failed to load administrative activity: {error}</span>
          <Button variant="outline" onClick={fetchEvents} icon={RefreshCw}>
            Retry
          </Button>
        </div>
      ) : loading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : events.length === 0 ? (
        <div className="flex flex-col items-center py-8 text-muted-foreground">
          <Search className="mb-2 size-8 opacity-50" />
          <p>No administrative activity found</p>
        </div>
      ) : (
        <>
          <p className="text-sm text-muted-foreground">
            {events.length} event{events.length === 1 ? "" : "s"} found
          </p>
          <div className="space-y-3 md:hidden">
            {events.map((event) => (
              <article
                key={event.id}
                data-testid="audit-event-mobile-card"
                className="rounded-lg border bg-card p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h3 className="truncate font-semibold">{event.actor}</h3>
                    <p className="mt-1 break-all text-xs text-muted-foreground">
                      {event.ip || "Unknown address"}
                    </p>
                  </div>
                  <Badge
                    variant={STATUS_BADGE_VARIANT[event.status] || "secondary"}
                  >
                    {event.status}
                  </Badge>
                </div>
                <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
                  <div>
                    <dt className="text-xs text-muted-foreground">Time</dt>
                    <dd className="mt-1">{formatTimestamp(event.timestamp)}</dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">Module</dt>
                    <dd className="mt-1">{event.module}</dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">Action</dt>
                    <dd className="mt-1">{event.action}</dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">Method</dt>
                    <dd className="mt-1">{event.method}</dd>
                  </div>
                </dl>
                <p className="mt-3 break-all text-xs text-muted-foreground">
                  {event.path}
                </p>
              </article>
            ))}
          </div>
          <div className="hidden overflow-x-auto md:block">
            <Table className="overflow-hidden rounded-lg border border-border">
              <TableHeader>
                <TableRow className="bg-muted hover:bg-muted">
                  <TableHead>Timestamp</TableHead>
                  <TableHead>Actor</TableHead>
                  <TableHead>Module</TableHead>
                  <TableHead>Action</TableHead>
                  <TableHead>Target</TableHead>
                  <TableHead>IP Address</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {events.map((event) => (
                  <TableRow key={event.id}>
                    <TableCell className="text-xs">
                      {formatTimestamp(event.timestamp)}
                    </TableCell>
                    <TableCell className="font-semibold">
                      {event.actor}
                      {event.actor_role ? (
                        <span className="ml-1 text-xs font-normal text-muted-foreground">
                          ({event.actor_role})
                        </span>
                      ) : null}
                    </TableCell>
                    <TableCell>{event.module}</TableCell>
                    <TableCell>{event.action}</TableCell>
                    <TableCell className="max-w-72 break-all text-xs text-muted-foreground">
                      <span className="font-medium text-foreground">
                        {event.method}
                      </span>{" "}
                      {event.path}
                    </TableCell>
                    <TableCell className="text-xs">
                      {event.ip || "-"}
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={
                          STATUS_BADGE_VARIANT[event.status] || "secondary"
                        }
                      >
                        {event.status}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </>
      )}
    </div>
  );
};

export default SystemAuditLogView;
