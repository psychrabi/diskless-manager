import { Button } from "@/components/ui/Button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/Table";
import { ChevronLeft, ChevronRight, Search } from "lucide-react";

const OPERATION_BADGE_CLASS = {
  shutdown: "badge-error",
  reboot: "badge-warning",
  remote: "badge-info",
};

const RESULT_BADGE_CLASS = {
  success: "badge-success",
  failed: "badge-error",
  timeout: "badge-warning",
  cancelled: "badge-neutral",
};

const getBadgeClass = (value, classMap) => {
  if (!value) return "badge-neutral";
  return classMap[value.toLowerCase()] || "badge-neutral";
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
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-sm text-base-content/70">
          {loading ? "Loading..." : `${logs.length} log entries found`}
        </p>
      </div>

      {loading ? (
        <div className="flex justify-center py-8">
          <span className="loading loading-spinner loading-lg"></span>
        </div>
      ) : logs.length === 0 ? (
        <div className="text-center py-8 text-base-content/60">
          <Search className="h-8 w-8 mx-auto mb-2 opacity-50" />
          <p>No audit logs found</p>
        </div>
      ) : (
        <>
          <Table className="border border-base-200 rounded-lg overflow-hidden">
            <TableHeader>
              <TableRow className="bg-base-200">
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
                <TableRow key={log.id} className="hover:bg-base-200/50">
                  <TableCell className="text-xs font-mono">
                    {formatTimestamp(log.timestamp)}
                  </TableCell>
                  <TableCell className="font-semibold">{log.client_name}</TableCell>
                  <TableCell className="text-xs font-mono">
                    {log.client_ip}
                  </TableCell>
                  <TableCell>
                    <span
                      className={`badge ${getBadgeClass(log.operation_type, OPERATION_BADGE_CLASS)}`}
                    >
                      {log.operation_type}
                    </span>
                  </TableCell>
                  <TableCell className="text-xs">{log.operation_mode || "-"}</TableCell>
                  <TableCell>
                    <span
                      className={`badge ${getBadgeClass(log.result, RESULT_BADGE_CLASS)}`}
                    >
                      {log.result}
                    </span>
                  </TableCell>
                  <TableCell className="text-xs font-mono">
                    {log.duration_ms || "-"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>

          {totalPages > 1 && (
            <div className="flex items-center justify-between py-4 border-t border-base-200">
              <p className="text-sm text-base-content/70">
                Page {currentPage} of {totalPages}
              </p>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onPreviousPage}
                  disabled={currentPage === 1}
                  icon={ChevronLeft}
                >
                  Previous
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={onNextPage}
                  disabled={currentPage === totalPages}
                  icon={ChevronRight}
                >
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
