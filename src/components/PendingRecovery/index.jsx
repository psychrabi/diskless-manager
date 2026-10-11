import { useCallback, useEffect, useState } from "react";
import { AlertCircle, RefreshCw, RotateCw } from "lucide-react";
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
        description="Retry interrupted client image and storage recovery. Clients must be disconnected before recovery can run."
        actions={(
          <Button variant="outline" size="sm" onClick={refresh} disabled={loading}>
            <RefreshCw data-icon="inline-start" /> Refresh
          </Button>
        )}
      />
      <Alert>
        <AlertCircle />
        <AlertTitle>Recovery keeps a journal until it finishes</AlertTitle>
        <AlertDescription>
          A failed retry stays pending and will continue to be retried automatically by the server.
        </AlertDescription>
      </Alert>

      {loadError && <Alert variant="destructive"><AlertTitle>Could not load recoveries</AlertTitle><AlertDescription>{loadError}</AlertDescription></Alert>}
      {loading ? (
        <div className="flex justify-center py-12"><Spinner className="size-8" /></div>
      ) : recoveries.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
          <p className="font-medium text-foreground">No pending client recoveries</p>
          <p className="mt-1 text-sm">Interrupted client storage operations will appear here.</p>
        </div>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border">
          <Table>
            <TableHeader>
              <TableRow className="bg-muted/50">
                <TableHead>Client</TableHead>
                <TableHead>Retry status</TableHead>
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
                      <Badge variant="secondary">
                        {recovery.failures} failed attempt{recovery.failures === 1 ? "" : "s"}
                      </Badge>
                      <span className="text-xs text-muted-foreground">{formatRetryTime(recovery.retry_after)}</span>
                    </div>
                  </TableCell>
                  <TableCell className="max-w-lg whitespace-normal break-words text-sm text-muted-foreground">
                    {recovery.last_error || "No error recorded"}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleRetry(recovery.client_id)}
                      disabled={retryingId !== null}
                    >
                      <RotateCw data-icon="inline-start" />
                      {retryingId === recovery.client_id ? "Retrying…" : "Retry now"}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
