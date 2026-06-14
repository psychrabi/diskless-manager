import { getLogs } from "@/api/modules/logs";
import { useToastStore } from "@/store/useToastStore";
import { useCallback, useState } from "react";

export const useLogs = () => {
  const [logs, setLogs] = useState(null);
  const { error } = useToastStore();

  const fetchLogs = useCallback(
    async (unit, lines = 50) => {
      if (!unit) return;
      try {
        console.log("Fetching logs for unit:", unit, "lines:", lines);
        const response = await getLogs(unit, lines);
        console.log("Logs response:", response);
        const logText = response?.text || response || "";
        console.log("Extracted log text:", logText);
        setLogs(logText);
      } catch (err) {
        console.error("Error fetching logs:", err);
        error(
          `Failed to fetch logs: ${
            err?.message || String(err) || "Unknown error"
          }`
        );
      }
    },
    [error]
  );

  return {
    logs,
    fetchLogs,
  };
};
