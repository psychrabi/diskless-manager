import { Layers, Monitor, Power, PowerOff, RefreshCw, Zap, MoveDown, MoveUp, Clock } from "lucide-react";
import React, { useState, useEffect } from "react";
import { TableVirtuoso } from "react-virtuoso";
import { useAppStore } from "../../store/useAppStore";
import { useMetrics } from "@/contexts/MetricsContext";
import { TableCell, TableHead } from "@/components/ui";
import { formatUptime } from "@/utils/formatUptime";
import ControlActionButtons from "./ControlActionButtons";

const ClientStatusBadge = React.memo(({ status }) => {
  const currentStatus = status || "Offline";
  const isOnline = currentStatus === "Online";
  const isLeased = currentStatus === "Leased";
  const badgeClass = isOnline
    ? "text-success"
    : isLeased
      ? "text-warning"
      : "text-neutral";
  return (
    <Monitor className={`inline mr-2 h-4 w-4 ${badgeClass}`} />  
  );
});

const ClientModeBadge = React.memo(({ client }) => {
  // Check if using master directly (no snapshot)
  const isUsingMasterDirectly = !client.snapshot || client.snapshot === "";
  
  if (isUsingMasterDirectly) {
    return (
      <span
        className="status status-warning status-lg"
        title={`Super Client : ${client.master}`}
      >
      </span>
    );
  }

  // Using a snapshot - check writeback mode
  if (client.keep_writeback === false) {
    return (
      <span
        className="status status-secondary status-lg"
        title={`Non-Persistent`}
      >
      
      </span>
    );
  }

  return (
    <span
      className="status status-info status-lg"
      title={`Persistent: ${client.block_store}`}
    >
      
    </span>
  );
});

// Define Virtuoso components outside to prevent remounting
const VirtuosoTableComponents = {
  Table: ({ style, ...props }, ref) => (
    <table
      {...props}
      className="table w-full bg-base-100"
      style={{ ...style }}
      ref={ref}
    />
  ),
  TableBody: React.forwardRef((props, ref) => <tbody {...props} ref={ref} />),
  TableRow: React.forwardRef((props, ref) => {
    // Destructure to avoid passing non-DOM props to tr, but capture item and context

    const { item, context, ...rest } = props;
    return (
      <tr
        {...rest}
        onContextMenu={(e) => {
          if (context?.handleClientContextMenu && item) {
            context.handleClientContextMenu(e, item);
          }
        }}
        className={`border-b border-base-200 transition-colors hover:bg-base-200/50 cursor-context-menu ${props.className || ""
          }`}
        ref={ref}
      />
    );
  }),
};

const ClientTable = ({ handleClientContextMenu }) => {
  const clients = useAppStore((state) => state.clients);
  const setClients = useAppStore((state) => state.setClients);
  const { metrics } = useMetrics();
  const [metricsMap, setMetricsMap] = useState({});

  // Build a map of client IP to metrics for quick lookup and update client statuses
  useEffect(() => {
    if (metrics?.clients) {
      const map = {};
      metrics.clients.forEach((metric) => {
        map[metric.ip] = metric;
      });
      setMetricsMap(map);

      // Update client statuses from metrics
      const updatedClients = clients.map((client) => {
        const metric = map[client.ip];
        if (metric && client.status !== metric.status) {
          return { ...client, status: metric.status };
        }
        return client;
      });

      // Only update if there are actual changes
      if (JSON.stringify(updatedClients) !== JSON.stringify(clients)) {
        setClients(updatedClients);
      }
    }
  }, [metrics, clients, setClients]);

  const getClientMetrics = (clientIp) => {
    return metricsMap[clientIp] || null;
  };

  return (
    <div className="bg-base-100 rounded-lg h-[calc(100vh-20rem)] w-full border border-base-200">
      {clients.length === 0 ? (
        <div className="p-4">
          <table className="table w-full">
            <thead>
              <tr className="border-b border-base-200">
                <TableHead >Name</TableHead>
                <TableHead className="hidden md:table-cell">
                  MAC Address
                </TableHead>
                <TableHead>IP Address</TableHead>
                    <TableHead className="hidden md:table-cell">Read <span className="hidden">(MB/s)</span></TableHead>
                <TableHead className="hidden md:table-cell" >Write (MB/s)</TableHead>
                <TableHead className="hidden md:table-cell">Image</TableHead>
                <TableHead className="hidden 2xl:table-cell">
                  Restore Point
                </TableHead>
                <TableHead className="hidden 2xl:table-cell">
                  Boot disk
                </TableHead>
                
                <TableHead className="hidden md:table-cell">Mode</TableHead>
                <TableHead className="hidden md:table-cell">Uptime</TableHead>
                <TableHead className="text-center">Actions</TableHead>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td
                  colSpan="8"
                  className="text-center py-4 text-base-content/60"
                >
                  No clients configured.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      ) : (
        <TableVirtuoso
          data={clients}
          context={{ handleClientContextMenu }}
          components={VirtuosoTableComponents}
          fixedHeaderContent={() => (
            <tr className="bg-base-100 border-b border-base-200 shadow-sm z-10 w-full text-center">
              <TableHead className="bg-base-100">Name</TableHead>
              <TableHead className="hidden md:table-cell bg-base-100 ">
                MAC Address
              </TableHead>
              <TableHead className="bg-base-100 ">IP Address</TableHead>
                    <TableHead className="bg-base-100 hidden lg:table-cell">Read <span className="hidden">(MB/s)</span></TableHead>
                    <TableHead className="bg-base-100 hidden lg:table-cell">Write <span className="hidden">(MB/s)</span></TableHead>
              <TableHead className="hidden 2xl:table-cell bg-base-100">
                Image
              </TableHead>
              <TableHead className="hidden 2xl:table-cell bg-base-100">
                Restore Point
              </TableHead>
              <TableHead className="hidden 2xl:table-cell bg-base-100">
                Boot disk
              </TableHead>
              
              <TableHead className="bg-base-100 lg:table-cell">Mode</TableHead>
              <TableHead className="hidden lg:table-cell bg-base-100">Uptime</TableHead>
              <TableHead className="bg-base-100 text-center">Actions</TableHead>
            </tr>
          )}
          itemContent={(_, client) => {
            const clientMetrics = getClientMetrics(client.ip);
            return (
              <>
                <TableCell className="font-bold font-mono">
                  <ClientStatusBadge status={client.status} />
                  {client.name}
                </TableCell>
                <TableCell className="hidden md:table-cell text-xs font-mono">
                  {client.mac}
                </TableCell>
                <TableCell className="font-mono text-xs">{client.ip}</TableCell>
                    <TableCell className="hidden lg:table-cell font-mono ">
                  {clientMetrics ? (
                    <span className="flex items-center justify-center gap-1">
                      <MoveUp className="w-3 h-3 text-info" />
                      {clientMetrics.read_speed_mbps.toFixed(2)}
                    </span>
                  ) : (
                    <span className="text-base-content/40">-</span>
                  )}
                </TableCell>
                <TableCell className="hidden lg:table-cell font-mono">
                  {clientMetrics ? (
                    <span className="flex items-center justify-center gap-1">
                      <MoveDown className="w-3 h-3 text-warning" />
                      {clientMetrics.write_speed_mbps.toFixed(2)}
                    </span>
                  ) : (
                    <span className="text-base-content/40">-</span>
                  )}
                </TableCell>
                <TableCell className="hidden xl:table-cell text-xs font-mono break-all">
                  {client.master}
                </TableCell>
                <TableCell className="hidden 2xl:table-cell text-xs font-mono break-all">
                  {client.snapshot ?? "-"}
                </TableCell>
                <TableCell className="hidden 2xl:table-cell text-xs font-mono break-all">
                  {client.block_store}
                </TableCell>
              
                <TableCell className="text-center">
                  <ClientModeBadge client={client} />
                </TableCell>
                <TableCell className="hidden lg:table-cell text-xs font-mono">
                  {clientMetrics ? (
                    <span className="flex items-center gap-1">
                      <Clock className="w-3 h-3 text-secondary" />
                      {formatUptime(clientMetrics.uptime_seconds)}
                    </span>
                  ) : (
                    <span className="text-base-content/40">-</span>
                  )}
                </TableCell>
                <TableCell className="text-center">
                  <ControlActionButtons client={client} onActionComplete={() => {}} />
                </TableCell>
              </>
            );
          }}
        />
      )}
    </div>
  );
};

export default ClientTable;
