import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

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
    <div className="p-4">
      <div className="flex items-center justify-between mb-2">
        <h2 className="text-lg font-semibold">Application Logs</h2>
        <div className="space-x-2">
          <button onClick={load} className="btn btn-ghost" disabled={loading}>
            {loading ? "Loading…" : "Refresh"}
          </button>
          <button onClick={clearLogs} className="btn btn-error" disabled={loading}>
            Clear
          </button>
        </div>
      </div>

      <div className="border rounded p-2 bg-base-200">
        <pre className="whitespace-pre-wrap break-words text-sm" style={{ maxHeight: "60vh", overflow: "auto" }}>
          {logs || "(no logs yet)"}
        </pre>
      </div>
    </div>
  );
}