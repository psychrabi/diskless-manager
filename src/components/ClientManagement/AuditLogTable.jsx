import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import { ChevronLeft, ChevronRight, Search } from "lucide-react";

const OPERATION_BADGE_VARIANT = {
  shutdown: "destructive",
  reboot: "outline",
  remote: "secondary",
};

const RESULT_BADGE_VARIANT = {
  success: "default",
  failed: "destructive",
  timeout: "outline",
  cancelled: "secondary",
};

const getBadgeVariant = (value, variantMap) => {
  if (!value) return "secondary";
  return variantMap[value.toLowerCase()] || "secondary";
};

const formatTimestamp = (timestamp) => {
  try {
    return new Date(timestamp).toLocaleString();
  } catch {
    return timestamp;
  }
};

const AuditLogTable = ({
  loading,
  logs,
  currentPage,
  totalPages,
  paginatedLogs,
  onPreviousPage,
  onNextPage,
}) => {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          {loading ? "Loading\u2026" : `${logs.length} log entries found`}
        </p>
      </div>

      {loading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : logs.length === 0 ? (
        <div className="flex flex-col items-center py-8 text-muted-foreground">
          <Search className="mb-2 size-8 opacity-50" />
          <p>No audit logs found</p>
        </div>
      ) : (
        <>
          <div className="space-y-3 md:hidden">
            {paginatedLogs.map((log) => (
              <article key={log.id} data-testid="audit-log-mobile-card" className="rounded-lg border bg-card p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h3 className="truncate font-semibold">{log.client_name}</h3>
                    <p className="mt-1 break-all text-xs text-muted-foreground">{log.client_ip}</p>
                  </div>
                  <Badge variant={getBadgeVariant(log.result, RESULT_BADGE_VARIANT)}>{log.result}</Badge>
                </div>
                <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
                  <div><dt className="text-xs text-muted-foreground">Time</dt><dd className="mt-1">{formatTimestamp(log.timestamp)}</dd></div>
                  <div><dt className="text-xs text-muted-foreground">Duration</dt><dd className="mt-1">{log.duration_ms || "-"} ms</dd></div>
                  <div><dt className="text-xs text-muted-foreground">Operation</dt><dd className="mt-1"><Badge variant={getBadgeVariant(log.operation_type, OPERATION_BADGE_VARIANT)}>{log.operation_type}</Badge></dd></div>
                  <div><dt className="text-xs text-muted-foreground">Mode</dt><dd className="mt-1">{log.operation_mode || "-"}</dd></div>
                </dl>
              </article>
            ))}
          </div>
          <div className="hidden overflow-x-auto md:block">
          <Table className="overflow-hidden rounded-lg border border-border">
            <TableHeader>
              <TableRow className="bg-muted hover:bg-muted">
                <TableHead>Timestamp</TableHead>
                <TableHead>Client</TableHead>
                <TableHead>IP Address</TableHead>
                <TableHead>Operation</TableHead>
                <TableHead>Mode</TableHead>
                <TableHead>Result</TableHead>
                <TableHead>Duration (ms)</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {paginatedLogs.map((log) => (
                <TableRow key={log.id}>
                  <TableCell className=" text-xs">
                    {formatTimestamp(log.timestamp)}
                  </TableCell>
                  <TableCell className="font-semibold">{log.client_name}</TableCell>
                  <TableCell className=" text-xs">
                    {log.client_ip}
                  </TableCell>
                  <TableCell>
                    <Badge variant={getBadgeVariant(log.operation_type, OPERATION_BADGE_VARIANT)}>
                      {log.operation_type}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-xs">{log.operation_mode || "-"}</TableCell>
                  <TableCell>
                    <Badge variant={getBadgeVariant(log.result, RESULT_BADGE_VARIANT)}>
                      {log.result}
                    </Badge>
                  </TableCell>
                  <TableCell className=" text-xs">
                    {log.duration_ms || "-"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          </div>

          {totalPages > 1 && (
            <div className="flex items-center justify-between border-t border-border py-4">
              <p className="text-sm text-muted-foreground">
                Page {currentPage} of {totalPages}
              </p>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onPreviousPage}
                  disabled={currentPage === 1}
                >
                  <ChevronLeft data-icon="inline-start" />
                  Previous
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onNextPage}
                  disabled={currentPage === totalPages}
                >
                  <ChevronRight data-icon="inline-start" />
                  Next
                </Button>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
};

export default AuditLogTable;
