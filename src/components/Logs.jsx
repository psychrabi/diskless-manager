import { NativeSelectOption } from "@/components/ui/native-select";
import { useLogs } from "@/hooks/useLogs";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useAppStore } from "../store/useAppStore";
import AppLogs from "./Logs/AppLogs";
import LogViewer from "./Logs/LogViewer";
import { Activity, Button, Card, PageHeader, Select } from "./ui";

const Logs = () => {
  const [logUnit, setLogUnit] = useState("app_log");
  const services = useAppStore((state) => state.services);
  const fetchServices = useAppStore((state) => state.fetchServices);
  const { logs, fetchLogs } = useLogs();

  useEffect(() => {
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
    <div className="flex items-center gap-2 rounded-xl border border-border/80 bg-card p-1 shadow-[0_1px_2px_rgb(0_0_0/0.03)]">
      <Select
        id="log-unit"
        value={logUnit}
        onChange={(e) => setLogUnit(e.target.value)}
        className="min-w-44 border-0 bg-transparent shadow-none"
      >
        <NativeSelectOption value="app_log">Show App logs</NativeSelectOption>
        {Array.isArray(services) && services.length > 0 ? (
          services.map((svc) => (
            <NativeSelectOption key={svc.name} value={svc.name}>
              {svc.display_name || svc.name}
            </NativeSelectOption>
          ))
        ) : (
          <NativeSelectOption disabled>No services available</NativeSelectOption>
        )}
      </Select>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={() => fetchLogs(logUnit)}
        title="Refresh Logs"
        icon={RefreshCw}
      />
    </div>
  );

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Logs"
        description="Server output and per-service diagnostics, without leaving the console."
        actions={logOptions}
      />
      <Activity mode={logUnit !== "app_log" ? "visible" : "hidden"}>
        <Card title={`${logUnit} Logs`}>
          <LogViewer content={logs} emptyText="" />
        </Card>
      </Activity>
      <Activity mode={logUnit === "app_log" ? "visible" : "hidden"}>
        <AppLogs />
      </Activity>
    </div>
  );
};

export default Logs;
