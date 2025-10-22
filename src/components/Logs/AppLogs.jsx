import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, Card } from "../ui";

export default function AppLogs({ tokenProp }) {
  const [logs, setLogs] = useState("");
  const [loading, setLoading] = useState(false);
  const token = tokenProp || localStorage.getItem("authToken") || "";

  async function load() {
    setLoading(true);
    try {
      const resp = await invoke("get_logs", { token });
      const text = resp && typeof resp === "object" && "text" in resp ? resp.text : String(resp ?? "");
      setLogs(text);
    } catch (e) {
      setLogs(`Error loading configuration:\n${e?.message ?? String(e)}`);
    } finally {
      setLoading(false);
    }
  }

  async function clearLogs() {
    setLoading(true);
    try {
      await invoke("clear_logs", { token });
      await load();
    } catch (e) {
      setLogs(`Error clearing logs:\n${e?.message ?? String(e)}`);
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Card title="Application Logs" className="bg-base-200" actions={<>
      <Button onClick={load} className="btn btn-info" disabled={loading}>
        {loading ? "Loading…" : "Refresh"}
      </Button>
      <Button onClick={clearLogs} className="btn btn-error" disabled={loading}>
        Clear
      </Button>
    </>}>
      <pre className="bg-base-300 p-3 rounded overflow-auto text-xs whitespace-pre-wrap max-h-[calc(100vh-20rem)]">
        {logs || "(no logs yet)"}
      </pre>
    </Card>
  );
}