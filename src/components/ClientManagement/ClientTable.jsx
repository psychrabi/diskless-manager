import { forwardRef, useEffect, useMemo } from "react";
import { TableVirtuoso } from "react-virtuoso";
import { useAppStore } from "../../store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";
import ClientTableEmptyState from "./ClientTableEmptyState";
import ClientTableHeader from "./ClientTableHeader";
import ClientTableRow from "./ClientTableRow";

const syncClientStatuses = (clients, metricsMap) => {
  let hasChanges = false;

  const updatedClients = clients.map((client) => {
    const nextStatus = metricsMap[client.ip]?.status;
    if (!nextStatus || nextStatus === client.status) {
      return client;
    }

    hasChanges = true;
    return { ...client, status: nextStatus };
  });

  return hasChanges ? updatedClients : clients;
};

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
  TableBody: forwardRef((props, ref) => <tbody {...props} ref={ref} />),
  TableRow: forwardRef((props, ref) => {
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

  const metricsMap = useMemo(() => {
    const map = {};
    metrics?.clients?.forEach((metric) => {
      map[metric.ip] = metric;
    });
    return map;
  }, [metrics]);

  useEffect(() => {
    if (!metrics?.clients) return;

    const updatedClients = syncClientStatuses(clients, metricsMap);
    if (updatedClients !== clients) {
      setClients(updatedClients);
    }
  }, [clients, metrics?.clients, metricsMap, setClients]);

  return (
    <div className="bg-base-100 rounded-lg h-[70vh] w-full border border-base-200 overflow-auto">
      {clients.length === 0 ? (
        <ClientTableEmptyState />
      ) : (
        <TableVirtuoso
          data={clients}
          context={{ handleClientContextMenu }}
          components={VirtuosoTableComponents}
          fixedHeaderContent={() => <ClientTableHeader fixed />}
          itemContent={(_, client) => {
            return (
              <ClientTableRow
                client={client}
                clientMetrics={metricsMap[client.ip] || null}
              />
            );
          }}
        />
      )}
    </div>
  );
};

export default ClientTable;
