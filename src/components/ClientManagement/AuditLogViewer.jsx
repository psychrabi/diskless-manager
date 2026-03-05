import { getAuditLogs } from "@/api/commands";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import { Select } from "@/components/ui/Select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/Table";
import { useToastStore } from "@/store/useToastStore";
import { ChevronLeft, ChevronRight, Filter, Search, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useAppStore } from "../../store/useAppStore";

const AuditLogViewer = ({ isOpen, onClose }) => {
  const clients = useAppStore((state) => state.clients);
  const { error } = useToastStore();

  // State for logs and filtering
  const [logs, setLogs] = useState([]);
  const [loading, setLoading] = useState(false);
  const [currentPage, setCurrentPage] = useState(1);
  const [itemsPerPage] = useState(10);

  // Filter state
  const [filters, setFilters] = useState({
    client_id: "",
    operation_type: "",
    start_date: "",
    end_date: "",
  });

  const fetchLogs = useCallback(async () => {
    try {
      setLoading(true);
      const response = await getAuditLogs(filters);
      setLogs(response.logs || []);
      setCurrentPage(1); // Reset to first page when fetching new data
    } catch (err) {
      error(`Failed to fetch audit logs: ${err.message}`, "error");
      setLogs([]);
    } finally {
      setLoading(false);
    }
  }, [error, filters]);

  // Fetch logs when modal opens or filters change
  useEffect(() => {
    if (isOpen) {
      fetchLogs();
    }
  }, [isOpen, fetchLogs]);

  const handleFilterChange = (field, value) => {
    setFilters((prev) => ({
      ...prev,
      [field]: value,
    }));
  };

  const handleClearFilters = () => {
    setFilters({
      client_id: "",
      operation_type: "",
      start_date: "",
      end_date: "",
    });
  };

  // Pagination
  const totalPages = Math.ceil(logs.length / itemsPerPage);
  const startIndex = (currentPage - 1) * itemsPerPage;
  const endIndex = startIndex + itemsPerPage;
  const paginatedLogs = logs.slice(startIndex, endIndex);

  const handlePreviousPage = () => {
    if (currentPage > 1) {
      setCurrentPage(currentPage - 1);
    }
  };

  const handleNextPage = () => {
    if (currentPage < totalPages) {
      setCurrentPage(currentPage + 1);
    }
  };

  // Format timestamp
  const formatTimestamp = (timestamp) => {
    try {
      return new Date(timestamp).toLocaleString();
    } catch {
      return timestamp;
    }
  };

  // Get operation type badge color
  const getOperationBadgeClass = (operationType) => {
    switch (operationType?.toLowerCase()) {
      case "shutdown":
        return "badge-error";
      case "reboot":
        return "badge-warning";
      case "remote":
        return "badge-info";
      default:
        return "badge-neutral";
    }
  };

  // Get result badge color
  const getResultBadgeClass = (result) => {
    switch (result?.toLowerCase()) {
      case "success":
        return "badge-success";
      case "failed":
        return "badge-error";
      case "timeout":
        return "badge-warning";
      case "cancelled":
        return "badge-neutral";
      default:
        return "badge-neutral";
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Audit Logs"
      size="5xl"
      className="max-h-[90vh] overflow-y-auto"
    >
      <div className="space-y-4">
        {/* Filters Section */}
        <div className="bg-base-200/30 p-4 rounded-lg space-y-3">
          <div className="flex items-center gap-2 mb-3">
            <Filter className="h-4 w-4" />
            <h3 className="font-semibold text-sm">Filters</h3>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
            {/* Client Filter */}
            <Select
              id="client-filter"
              label="Client"
              value={filters.client_id}
              onChange={(e) => handleFilterChange("client_id", e.target.value)}
            >
              <option value="">All Clients</option>
              {clients.map((client) => (
                <option key={client.id} value={client.id}>
                  {client.name}
                </option>
              ))}
            </Select>

            {/* Operation Type Filter */}
            <Select
              id="operation-filter"
              label="Operation Type"
              value={filters.operation_type}
              onChange={(e) =>
                handleFilterChange("operation_type", e.target.value)
              }
            >
              <option value="">All Operations</option>
              <option value="shutdown">Shutdown</option>
              <option value="reboot">Reboot</option>
              <option value="remote">Remote Desktop</option>
            </Select>

            {/* Start Date Filter */}
            <div>
              <label htmlFor="start-date" className="form-label">
                Start Date
              </label>
              <input
                id="start-date"
                type="date"
                value={filters.start_date}
                onChange={(e) =>
                  handleFilterChange("start_date", e.target.value)
                }
                className="input w-full"
              />
            </div>

            {/* End Date Filter */}
            <div>
              <label htmlFor="end-date" className="form-label">
                End Date
              </label>
              <input
                id="end-date"
                type="date"
                value={filters.end_date}
                onChange={(e) => handleFilterChange("end_date", e.target.value)}
                className="input w-full"
              />
            </div>
          </div>

          {/* Filter Actions */}
          <div className="flex gap-2 justify-end">
            <Button
              variant="ghost"
              size="sm"
              onClick={handleClearFilters}
              icon={X}
            >
              Clear Filters
            </Button>
          </div>
        </div>

        {/* Results Section */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <p className="text-sm text-base-content/70">
              {loading ? "Loading..." : `${logs.length} log entries found`}
            </p>
          </div>

          {/* Table */}
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
                      <TableCell className="font-semibold">
                        {log.client_name}
                      </TableCell>
                      <TableCell className="text-xs font-mono">
                        {log.client_ip}
                      </TableCell>
                      <TableCell>
                        <span
                          className={`badge ${getOperationBadgeClass(log.operation_type)}`}
                        >
                          {log.operation_type}
                        </span>
                      </TableCell>
                      <TableCell className="text-xs">
                        {log.operation_mode || "-"}
                      </TableCell>
                      <TableCell>
                        <span
                          className={`badge ${getResultBadgeClass(log.result)}`}
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

              {/* Pagination */}
              {totalPages > 1 && (
                <div className="flex items-center justify-between py-4 border-t border-base-200">
                  <p className="text-sm text-base-content/70">
                    Page {currentPage} of {totalPages}
                  </p>
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handlePreviousPage}
                      disabled={currentPage === 1}
                      icon={ChevronLeft}
                    >
                      Previous
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleNextPage}
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
      </div>
    </Modal>
  );
};

export default AuditLogViewer;
