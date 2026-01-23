import React, { useState, useEffect } from "react";
import { Clock, X, AlertCircle, Trash2 } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { useNotification } from "../../contexts/notification";
import { Button } from "@/components/ui/Button";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/Table";
import { Modal } from "@/components/ui/Modal";
import { cancelScheduledOperation, getScheduledOperations } from "@/api/commands";
import { useToastStore } from "@/store/useToastStore";

const ScheduledOperationsList = ({ isOpen, onClose }) => {
  const clients = useAppStore((state) => state.clients);
  const { error, success } = useToastStore();

  // State for scheduled operations
  const [operations, setOperations] = useState([]);
  const [loading, setLoading] = useState(false);
  const [cancellingId, setCancellingId] = useState(null);

  // Fetch scheduled operations when modal opens
  useEffect(() => {
    if (isOpen) {
      fetchScheduledOperations();
      // Refresh every 10 seconds
      const interval = setInterval(fetchScheduledOperations, 10000);
      return () => clearInterval(interval);
    }
  }, [isOpen]);

  const fetchScheduledOperations = async () => {
    try {
      setLoading(true);
      const response = await getScheduledOperations();
      setOperations(response.operations || []);
    } catch (err) {
      error(`Failed to fetch scheduled operations: ${err.message}`);
      setOperations([]);
    } finally {
      setLoading(false);
    }
  };

  const handleCancelOperation = async (operationId) => {
    if (!window.confirm("Are you sure you want to cancel this scheduled operation?")) {
      return;
    }

    try {
      setCancellingId(operationId);
      await cancelScheduledOperation(operationId);
      success("Scheduled operation cancelled successfully");
      await fetchScheduledOperations();
    } catch (err) {
      error(`Failed to cancel operation: ${err.message}`);
    } finally {
      setCancellingId(null);
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
      default:
        return "badge-neutral";
    }
  };

  // Get operation mode badge color
  const getModeBadgeClass = (mode) => {
    switch (mode?.toLowerCase()) {
      case "graceful":
        return "badge-info";
      case "force":
        return "badge-error";
      default:
        return "badge-neutral";
    }
  };

  // Get client name by ID
  const getClientName = (clientId) => {
    const client = clients.find((c) => c.id === clientId);
    return client ? client.name : `Unknown (${clientId})`;
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Scheduled Operations"
      size="4xl"
      className="max-h-[90vh] overflow-y-auto"
    >
      <div className="space-y-4">
        {/* Info Banner */}
        <div className="bg-info/10 border border-info/30 p-4 rounded-lg flex gap-3">
          <AlertCircle className="h-5 w-5 text-info flex-shrink-0 mt-0.5" />
          <div className="text-sm text-info-content">
            <p className="font-semibold">Scheduled Operations</p>
            <p className="text-xs mt-1">
              View and manage operations scheduled to run on clients. Operations will execute at their scheduled time.
            </p>
          </div>
        </div>

        {/* Results Section */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <p className="text-sm text-base-content/70">
              {loading ? "Loading..." : `${operations.length} scheduled operation${operations.length !== 1 ? "s" : ""}`}
            </p>
          </div>

          {/* Table */}
          {loading ? (
            <div className="flex justify-center py-8">
              <span className="loading loading-spinner loading-lg"></span>
            </div>
          ) : operations.length === 0 ? (
            <div className="text-center py-12 text-base-content/60">
              <Clock className="h-12 w-12 mx-auto mb-3 opacity-30" />
              <p className="font-medium">No Scheduled Operations</p>
              <p className="text-xs mt-1">
                Scheduled operations will appear here when you schedule shutdown or reboot operations with a delay.
              </p>
            </div>
          ) : (
            <Table className="border border-base-200 rounded-lg overflow-hidden">
              <TableHeader>
                <TableRow className="bg-base-200">
                  <TableHead>Client</TableHead>
                  <TableHead>Operation</TableHead>
                  <TableHead>Mode</TableHead>
                  <TableHead>Scheduled Time</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {operations.map((operation) => (
                  <TableRow key={operation.id} className="hover:bg-base-200/50">
                    <TableCell className="font-semibold">
                      {getClientName(operation.client_id)}
                    </TableCell>
                    <TableCell>
                      <span className={`badge ${getOperationBadgeClass(operation.operation_type)}`}>
                        {operation.operation_type}
                      </span>
                    </TableCell>
                    <TableCell>
                      <span className={`badge ${getModeBadgeClass(operation.operation_mode)}`}>
                        {operation.operation_mode}
                      </span>
                    </TableCell>
                    <TableCell className="text-xs font-mono">
                      {formatTimestamp(operation.scheduled_time)}
                    </TableCell>
                    <TableCell>
                      <span className="badge badge-outline">
                        {operation.result ? operation.result : "Pending"}
                      </span>
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={Trash2}
                        onClick={() => handleCancelOperation(operation.id)}
                        disabled={cancellingId === operation.id}
                        className="text-error hover:text-error"
                      >
                        {cancellingId === operation.id ? "Cancelling..." : "Cancel"}
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </div>
      </div>
    </Modal>
  );
};

export default ScheduledOperationsList;
