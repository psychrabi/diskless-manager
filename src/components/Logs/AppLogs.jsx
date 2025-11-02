import { useNotification } from '@/contexts/notification';
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, Card } from "../ui";
import { Brush, BrushCleaning, Loader, RefreshCw } from 'lucide-react';

export default function AppLogs({ tokenProp }) {
  const [logs, setLogs] = useState("");
  const [loading, setLoading] = useState(false);
  const token = tokenProp || localStorage.getItem("authToken") || "";
  const { showNotification } = useNotification();

  async function load() {
    setLoading(true);
    try {
      const resp = await invoke("get_logs", { token });
      const text = resp && typeof resp === "object" && "text" in resp ? resp.text : String(resp ?? "");
      setLogs(text);
    } catch (e) {
      showNotification('error', 'Failed to load logs', e?.message ?? String(e));
      setLogs(''); // Clear logs on error
    } finally {
      setLoading(false);
    }
  }

  async function clearLogs() {
    setLoading(true);
    try {
      await invoke("clear_logs", { token });
      showNotification('success', 'Logs cleared', 'Application logs have been cleared successfully.');
      await load();
    } catch (e) {
      showNotification('error', 'Failed to clear logs', e?.message ?? String(e));
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Card title="Application Logs" className="bg-base-200" headerClass="p-4" bodyClass="border-t-1 p-0" actions={<>
      <Button onClick={load} className="btn btn-info btn-sm" disabled={loading}>
        {loading ? <Loader className="animate-spin" size={16} /> : <RefreshCw size={16} />}
      </Button>
      <Button onClick={clearLogs} className="btn btn-error btn-sm" disabled={loading}>
        <BrushCleaning size={16} />
      </Button>
    </>}>
      <pre className="bg-base-300 p-2 rounded overflow-auto text-xs whitespace-pre-wrap max-h-[calc(100vh-20rem)]">
        {logs || "(no logs yet)"}
      </pre>
    </Card>
  );
}