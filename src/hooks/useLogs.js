import { useToastStore } from "@/store/useToastStore";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export const useLogs = () => {
  const [logs, setLogs] = useState(null);
  const [loading, setLoading] = useState(false);
  const { error } = useToastStore();

  const fetchLogs = useCallback(
    async (unit, lines = 50) => {
      if (!unit) return;
      setLoading(true);
      try {
        const out = await invoke("get_service_logs", {
          serviceName: unit,
          lines,
        });
        setLogs(out);
      } catch (err) {
        console.error(err);
        error(
          `Failed to fetch logs: ${
            err?.message || String(err) || "Unknown error"
          }`
        );
      } finally {
        setLoading(false);
      }
    },
    [error]
  );

  return {
    logs,
    loading,
    fetchLogs,
  };
};
