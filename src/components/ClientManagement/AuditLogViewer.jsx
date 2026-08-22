import { getAuditLogs } from "@/api/modules/control";
import { Modal } from "@/components/ui/Modal";
import { useToastStore } from "@/store/useToastStore";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppStore } from "../../store/useAppStore";
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
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Audit Logs"
      size="5xl"
      className="max-h-[90vh] overflow-y-auto"
    >
      <div className="space-y-4">
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
    </Modal>
  );
};

export default AuditLogViewer;
