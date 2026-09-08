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
                  <TableCell className="font-mono text-xs">
                    {formatTimestamp(log.timestamp)}
                  </TableCell>
                  <TableCell className="font-semibold">{log.client_name}</TableCell>
                  <TableCell className="font-mono text-xs">
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
                  <TableCell className="font-mono text-xs">
                    {log.duration_ms || "-"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>

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
