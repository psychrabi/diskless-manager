import { useCallback, useEffect, useState } from "react";
import { AlertCircle, CircleCheck, RefreshCw, RotateCw } from "lucide-react";
import { getPendingClientRecoveries, retryPendingClientRecovery } from "@/api/modules/reconciliation";
import { Button, PageHeader } from "@/components/ui";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useToastStore } from "@/store/useToastStore";

function formatRetryTime(timestamp) {
  if (!timestamp) return "Ready for automatic retry";
  return `Automatic retry after ${new Date(timestamp * 1000).toLocaleString()}`;
}

export default function PendingRecovery() {
  const [recoveries, setRecoveries] = useState([]);
  const [loading, setLoading] = useState(true);
  const [retryingId, setRetryingId] = useState(null);
  const [loadError, setLoadError] = useState("");
  const { error, success } = useToastStore();

  const refresh = useCallback(async () => {
    try {
      const response = await getPendingClientRecoveries();
      setRecoveries(response.recoveries || []);
      setLoadError("");
    } catch (err) {
      setLoadError(err.message || "Failed to load pending recoveries");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = setTimeout(refresh, 0);
    const interval = setInterval(refresh, 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [refresh]);

  const handleRetry = async (clientId) => {
    setRetryingId(clientId);
    try {
      await retryPendingClientRecovery(clientId);
      success("Client storage recovery completed");
      await refresh();
    } catch (err) {
      error(`Recovery retry failed: ${err.message}`);
      await refresh();
    } finally {
      setRetryingId(null);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Pending Recovery"
        description="Review boot provisioning failures and retry interrupted client storage recovery."
        actions={(
          <Button variant="outline" size="sm" className="max-sm:flex-none" icon={RefreshCw} onClick={refresh} disabled={loading}>
            Refresh
          </Button>
        )}
      />
      {loadError && <Alert variant="destructive"><AlertTitle>Could not load recoveries</AlertTitle><AlertDescription>{loadError}</AlertDescription></Alert>}
      {loading ? (
        <div className="flex justify-center py-12"><Spinner className="size-8" /></div>
      ) : loadError && recoveries.length === 0 ? null : recoveries.length === 0 ? (
        <Alert className="bg-card">
          <CircleCheck className="text-success" />
          <AlertTitle>No pending recovery or boot failures</AlertTitle>
          <AlertDescription>Interrupted storage operations and boot provisioning failures will appear here.</AlertDescription>
        </Alert>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border">
          <Table>
            <TableHeader>
              <TableRow className="bg-muted/50">
                <TableHead>Client</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Last error</TableHead>
                <TableHead className="text-right">Action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {recoveries.map((recovery) => (
                <TableRow key={recovery.client_id}>
                  <TableCell className="font-medium">{recovery.client_name}</TableCell>
                  <TableCell>
                    <div className="flex flex-col items-start gap-1">
                      <Badge variant="outline">
                        {recovery.kind === "boot_failure" ? "Boot provisioning" : "Storage recovery"}
                      </Badge>
                      <Badge variant="secondary">
                        {recovery.failures} failed attempt{recovery.failures === 1 ? "" : "s"}
                      </Badge>
                      <span className="text-xs text-muted-foreground">
                        {recovery.kind === "boot_failure" ? "Next PXE boot retries provisioning" : formatRetryTime(recovery.retry_after)}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell className="max-w-lg whitespace-normal break-words text-sm text-muted-foreground">
                    {recovery.last_error || "No error recorded"}
                  </TableCell>
                  <TableCell className="text-right">
                    {recovery.kind !== "boot_failure" && (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleRetry(recovery.client_id)}
                        disabled={retryingId !== null}
                      >
                        <RotateCw data-icon="inline-start" />
                        {retryingId === recovery.client_id ? "Retrying…" : "Retry now"}
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {recoveries.length > 0 && <Alert>
        <AlertCircle />
            <AlertTitle>Failures stay visible until resolved</AlertTitle>
        <AlertDescription>
              Storage recovery retries automatically. Boot provisioning retries on the client's next PXE boot.
        </AlertDescription>
      </Alert>}
    </div>
  );
}
