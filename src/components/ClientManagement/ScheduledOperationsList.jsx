import { useCallback, useEffect, useState } from "react";
import { AlertCircle, Clock, Trash2 } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@/components/ui/table";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import { Spinner } from "@/components/ui/spinner";
import { Modal } from "@/components/ui/Modal";
import { cancelScheduledOperation, getScheduledOperations } from "@/api/modules/control";
import { useToastStore } from "@/store/useToastStore";

const getOperationBadgeVariant = (operationType) => {
  switch (operationType?.toLowerCase()) {
    case "shutdown":
      return "destructive";
    case "reboot":
      return "outline";
    default:
      return "secondary";
  }
};

const getModeBadgeVariant = (mode) => {
  switch (mode?.toLowerCase()) {
    case "graceful":
      return "outline";
    case "force":
      return "destructive";
    default:
      return "secondary";
  }
};

const ScheduledOperationsList = ({ isOpen, onClose }) => {
  const clients = useAppStore((state) => state.clients);
  const { error, success } = useToastStore();

  // State for scheduled operations
  const [operations, setOperations] = useState([]);
  const [loading, setLoading] = useState(false);
  const [cancellingId, setCancellingId] = useState(null);

  const fetchScheduledOperations = useCallback(async () => {
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
  }, [error]);

  // Fetch scheduled operations when modal opens
  useEffect(() => {
    if (!isOpen) return undefined;
    // Defer initial fetch so setState is not synchronous within the
    // effect body (react-hooks/set-state-in-effect).
    const timer = setTimeout(fetchScheduledOperations, 0);
    // Refresh every 10 seconds
    const interval = setInterval(fetchScheduledOperations, 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [isOpen, fetchScheduledOperations]);

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
      <div className="flex flex-col gap-4">
        {/* Info Banner */}
        <Alert>
          <AlertCircle />
          <AlertTitle>Scheduled Operations</AlertTitle>
          <AlertDescription>
            View and manage operations scheduled to run on clients. Operations
            will execute at their scheduled time.
          </AlertDescription>
        </Alert>

        {/* Results Section */}
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              {loading ? "Loading\u2026" : `${operations.length} scheduled operation${operations.length !== 1 ? "s" : ""}`}
            </p>
          </div>

          {/* Table */}
          {loading ? (
            <div className="flex justify-center py-8">
              <Spinner className="size-8" />
            </div>
          ) : operations.length === 0 ? (
            <div className="py-12 text-center text-muted-foreground/60">
              <Clock className="mx-auto mb-3 size-12 opacity-30" />
              <p className="font-medium">No Scheduled Operations</p>
              <p className="mt-1 text-xs">
                Scheduled operations will appear here when you schedule shutdown or reboot operations with a delay.
              </p>
            </div>
          ) : (
            <Table className="overflow-hidden rounded-lg border border-border">
              <TableHeader>
                <TableRow className="bg-muted/50">
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
                  <TableRow key={operation.id} className="hover:bg-muted/50">
                    <TableCell className="font-semibold">
                      {getClientName(operation.client_id)}
                    </TableCell>
                    <TableCell>
                      <Badge variant={getOperationBadgeVariant(operation.operation_type)}>
                        {operation.operation_type}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge variant={getModeBadgeVariant(operation.operation_mode)}>
                        {operation.operation_mode}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-xs font-mono">
                      {formatTimestamp(operation.scheduled_time)}
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {operation.result ? operation.result : "Pending"}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleCancelOperation(operation.id)}
                        disabled={cancellingId === operation.id}
                      >
                        <Trash2 data-icon="inline-start" />
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
