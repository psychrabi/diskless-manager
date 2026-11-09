import { forwardRef, useEffect, useMemo, useState } from "react";
import { TableVirtuoso } from "react-virtuoso";
import { Search, SearchX } from "lucide-react";
import { useAppStore } from "../../store/useAppStore";
import { useMetrics } from "@/contexts/useMetrics";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { isOnlineStatus } from "@/lib/clientStatus";
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
      className="w-full min-w-0 bg-card xl:min-w-[1000px]"
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
        className={`group border-b border-border/60 transition-colors hover:bg-muted/30 cursor-context-menu ${props.className || ""
          }`}
        ref={ref}
      />
    );
  }),
};

const FILTERS = [
  { id: "all", label: "All" },
  { id: "online", label: "Online" },
  { id: "offline", label: "Offline" },
];

const ClientTable = ({ handleClientContextMenu }) => {
  const clients = useAppStore((state) => state.clients);
  const setClients = useAppStore((state) => state.setClients);
  const { metrics } = useMetrics();
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const scrollParent = document.getElementById("main-content");

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

  const shownClients = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return clients.filter((client) => {
      const online = isOnlineStatus(client.status);
      if (statusFilter === "online" && !online) return false;
      if (statusFilter === "offline" && online) return false;
      if (!needle) return true;
      return [client.name, client.mac, client.ip, client.master]
        .some((value) => String(value ?? "").toLowerCase().includes(needle));
    });
  }, [clients, query, statusFilter]);

  return (
    <div className="overflow-hidden rounded-2xl border border-border/80 bg-card shadow-[0_1px_2px_rgb(0_0_0/0.035)]">
      <div className="flex flex-col gap-2 border-b border-border/70 p-3 sm:flex-row sm:items-center sm:justify-between sm:gap-3 sm:p-4">
        <div className="relative w-full sm:max-w-xs">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search clients, MAC, IP, image…"
            aria-label="Search clients"
            className="h-11 rounded-lg border-border/80 bg-background pl-9 text-[12px] shadow-none xl:h-9"
          />
        </div>
        <div className="flex flex-wrap items-center gap-1">
          {FILTERS.map((filter) => (
            <button
              key={filter.id}
              type="button"
              onClick={() => setStatusFilter(filter.id)}
              aria-pressed={statusFilter === filter.id}
              className={cn(
                "h-11 rounded-lg px-2.5 text-[11px] font-medium transition-colors xl:h-8",
                statusFilter === filter.id
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              )}
            >
              {filter.label}
            </button>
          ))}
          <span className="ml-1 text-[11px] tabular-nums text-muted-foreground sm:ml-2">
            {shownClients.length} of {clients.length}
          </span>
        </div>
      </div>

      <div className="w-full">
        {clients.length === 0 ? (
          <ClientTableEmptyState />
        ) : shownClients.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center">
            <span className="flex size-11 items-center justify-center rounded-full border border-border/80 bg-muted/40">
              <SearchX className="size-5 text-muted-foreground" aria-hidden="true" />
            </span>
            <p className="mt-3 text-sm font-medium">No clients match this view</p>
            <p className="mt-1 max-w-xs text-xs text-muted-foreground">
              Try a different status filter or clear the search term.
            </p>
          </div>
        ) : (
          scrollParent && (
            <TableVirtuoso
              customScrollParent={scrollParent}
              data={shownClients}
              context={{ handleClientContextMenu }}
              components={VirtuosoTableComponents}
              fixedHeaderContent={() => <ClientTableHeader fixed />}
              itemContent={(_, client) => (
                <ClientTableRow
                  client={client}
                  clientMetrics={metricsMap[client.ip] || null}
                  onMoreActions={handleClientContextMenu}
                />
              )}
            />
          )
        )}
      </div>
    </div>
  );
};

export default ClientTable;
