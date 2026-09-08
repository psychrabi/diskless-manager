import { getAuditLogs } from "@/api/modules/control";
import { useToastStore } from "@/store/useToastStore";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppStore } from "../../store/useAppStore";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import AuditLogFilters from "./AuditLogFilters";
import AuditLogTable from "./AuditLogTable";

const DEFAULT_FILTERS = {
  client_id: "",
  operation_type: "",
  start_date: "",
  end_date: "",
};

const ITEMS_PER_PAGE = 10;

const AuditLogViewer = ({ isOpen, onClose }) => {
  const clients = useAppStore((state) => state.clients);
  const { error } = useToastStore();

  const [logs, setLogs] = useState([]);
  const [loading, setLoading] = useState(false);
  const [currentPage, setCurrentPage] = useState(1);
  const [filters, setFilters] = useState(DEFAULT_FILTERS);

  const fetchLogs = useCallback(async () => {
    try {
      setLoading(true);
      const response = await getAuditLogs(filters);
      setLogs(response.logs || []);
      setCurrentPage(1);
    } catch (err) {
      error("Audit Logs", `Failed to fetch audit logs: ${err.message || err}`);
      setLogs([]);
    } finally {
      setLoading(false);
    }
  }, [error, filters]);

  useEffect(() => {
    if (!isOpen) return undefined;
    // Defer so setState inside fetchLogs is not synchronous within
    // the effect body (react-hooks/set-state-in-effect).
    const timer = setTimeout(fetchLogs, 0);
    return () => clearTimeout(timer);
  }, [isOpen, fetchLogs]);

  const handleFilterChange = useCallback((field, value) => {
    setFilters((prev) => ({
      ...prev,
      [field]: value,
    }));
  }, []);

  const handleClearFilters = useCallback(() => {
    setFilters(DEFAULT_FILTERS);
  }, []);

  const totalPages = useMemo(
    () => Math.ceil(logs.length / ITEMS_PER_PAGE),
    [logs.length]
  );

  const paginatedLogs = useMemo(() => {
    const startIndex = (currentPage - 1) * ITEMS_PER_PAGE;
    return logs.slice(startIndex, startIndex + ITEMS_PER_PAGE);
  }, [currentPage, logs]);

  const handlePreviousPage = useCallback(() => {
    setCurrentPage((prev) => Math.max(1, prev - 1));
  }, []);

  const handleNextPage = useCallback(() => {
    setCurrentPage((prev) => Math.min(totalPages, prev + 1));
  }, [totalPages]);

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="max-h-[95vh] overflow-y-auto sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>Audit Logs</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <AuditLogFilters
            filters={filters}
            clients={clients}
            onFilterChange={handleFilterChange}
            onClearFilters={handleClearFilters}
          />

          <AuditLogTable
            loading={loading}
            logs={logs}
            currentPage={currentPage}
            totalPages={totalPages}
            paginatedLogs={paginatedLogs}
            onPreviousPage={handlePreviousPage}
            onNextPage={handleNextPage}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default AuditLogViewer;
