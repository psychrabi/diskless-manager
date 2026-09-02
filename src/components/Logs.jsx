import { useLogs } from "@/hooks/useLogs";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useAppStore } from "../store/useAppStore";
import AppLogs from "./Logs/AppLogs";
import LogViewer from "./Logs/LogViewer";
import { Activity, Button, Card, Select } from "./ui";

const Logs = () => {
  const [logUnit, setLogUnit] = useState("app_log");
  const services = useAppStore((state) => state.services);
  const fetchServices = useAppStore((state) => state.fetchServices);
  const { logs, fetchLogs } = useLogs();

  useEffect(() => {
    console.log("Services in Logs component:", services);
    // Fetch services if not already loaded
    if (!services || services.length === 0) {
      fetchServices();
    }
  }, [fetchServices, services]);

  useEffect(() => {
    if (logUnit) {
      fetchLogs(logUnit);
    }
  }, [logUnit, fetchLogs]);

  const logOptions = (
    <div className="flex gap-2">
      <Select
        id="log-unit"
        value={logUnit}
        onChange={(e) => setLogUnit(e.target.value)}
      >
        <option value="app_log">Show App logs</option>
        {Array.isArray(services) && services.length > 0 ? (
          services.map((svc) => (
            <option key={svc.name} value={svc.name}>
              {svc.display_name || svc.name}
            </option>
          ))
        ) : (
          <option disabled>No services available</option>
        )}
      </Select>
      <Button
        variant="ghost"
        size="icon"
        onClick={() => fetchLogs(logUnit)}
        title="Refresh Logs"
        icon={RefreshCw}
      />
    </div>
  );

  return (
    <Card
      title="Logs"
      headerClass="p-4"
      actions={logOptions}
      className="max-h-[calc(100vh-7rem)]"
    >
      <Activity mode={logUnit !== "app_log" ? "visible" : "hidden"}>
        <Card
          title={`${logUnit} Logs`}
          className="bg-base-200"
          headerClass="p-4"
          bodyClass="border-t-1"
        >
          <LogViewer content={logs} emptyText="" />
        </Card>
      </Activity>
      <Activity mode={logUnit === "app_log" ? "visible" : "hidden"}>
        <AppLogs />
      </Activity>
    </Card>
  );
};

export default Logs;
