import { useToastStore } from "@/store/useToastStore";
import { getLogs, clearLogs } from "@/api/modules/logs";
import { BrushCleaning, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Card } from "@/components/ui";
import LogViewer from "./LogViewer";

export default function AppLogs() {
  const [logs, setLogs] = useState("");
  const [loading, setLoading] = useState(false);
  const { success, error } = useToastStore();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await getLogs();
      const text =
        resp && typeof resp === "object" && "text" in resp
          ? resp.text
          : String(resp ?? "");
      setLogs(text);
    } catch (e) {
      error(`Failed to load logs ${e?.message ?? String(e)}`);
      setLogs(""); // Clear logs on error
    } finally {
      setLoading(false);
    }
  }, [error]);

  async function handleClearLogs() {
    setLoading(true);
    try {
      await clearLogs();
      success("Logs", "Application logs have been cleared successfully.");
      await load();
    } catch (e) {
      error(`Failed to clear logs ${e?.message ?? String(e)}`);
      setLoading(false);
    }
  }

  useEffect(() => {
    // Defer so setState inside load() is not synchronous within the
    // effect body (react-hooks/set-state-in-effect).
    const timer = setTimeout(load, 0);
    return () => clearTimeout(timer);
  }, [load]);

  return (
    <Card
      title="Application Logs"
      className="bg-base-200"
      headerClass="p-4"
      bodyClass="border-t-1 p-0"
      actions={
        <>
          <Button
            onClick={load}
            variant="info"
            size="sm"
            icon={RefreshCw}
            loading={loading}
            title="Refresh logs"
          />
          <Button
            onClick={handleClearLogs}
            variant="destructive"
            size="sm"
            icon={BrushCleaning}
            disabled={loading}
            title="Clear logs"
          />
        </>
      }
    >
      <LogViewer content={logs} />
    </Card>
  );
}
