import { useClientActions } from "@/hooks/useClientActions";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { LoadingSkeleton } from "@/components/ui/LoadingSkeleton";
import { ContextMenu } from "../ui/ContextMenu";
import { Laptop, PlusCircle, Users, Wifi, WifiOff, History, Clock } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "../../store/useAppStore";
import ClientFormModal from "./ClientFormModal";
import ClientTable from "./ClientTable";
import ClientHero from "./ClientHero";
import RemoteDesktopModal from "./RemoteDesktopModal";
import PowerActionModal from "./PowerActionModal";
import AuditLogViewer from "./AuditLogViewer";
import ScheduledOperationsList from "./ScheduledOperationsList";

const MemoizedClientTable = memo(ClientTable);
const MemoizedContextMenu = memo(ContextMenu);

const ClientManagement = () => {
  const {
    clients,
    fetchClients,
    fetchImages,
    masters,
    loading,
  } = useAppStore(
    useShallow((state) => ({
      clients: state.clients,
      fetchClients: state.fetchClients,
      fetchImages: state.fetchImages,
      masters: state.masters,
      loading: state.loading,
    })),
  );

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isRemoteModalOpen, setIsRemoteModalOpen] = useState(false);
  const [powerAction, setPowerAction] = useState(null);
  const [isAuditLogViewerOpen, setIsAuditLogViewerOpen] = useState(false);
  const [isScheduledOperationsOpen, setIsScheduledOperationsOpen] = useState(false);
  const [client, setClient] = useState({
    name: "",
    mac: "",
    ip: "",
    master: "",
    snapshot: "",
    clone: "",
  });
  const [contextMenu, setContextMenu] = useState({
    isOpen: false,
    x: 0,
    y: 0,
    client: null,
  });

  const handleClientContextMenu = useCallback((event, client) => {
    event.preventDefault();
    setContextMenu({
      isOpen: true,
      x: event.clientX,
      y: event.clientY,
      client: client,
    });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu((prev) => ({ ...prev, isOpen: false }));
  }, []);

  // refreshData callback for actions
  const refreshData = useCallback(async () => {
    await Promise.all([fetchClients(), fetchImages()]);
  }, [fetchClients, fetchImages]);

  const contextActions = useClientActions(
    refreshData,
    closeContextMenu,
    setClient,
    setIsModalOpen,
    () => setIsRemoteModalOpen(true),
    setPowerAction,
  );

  const handleClientFormModalOpen = useCallback(() => {
    let newName = "PC001";
    let newIp = "192.168.1.101"; // Default start IP

    if (clients.length > 0) {
      // Sort clients by name to find the "last" one logically
      const sortedClients = [...clients].sort((a, b) =>
        a.name.localeCompare(b.name)
      );
      const lastClient = sortedClients[sortedClients.length - 1];

      // Extract number from last client name and increment
      const nameMatch = lastClient.name.match(/(\d+)$/);
      if (nameMatch) {
        const lastNumber = parseInt(nameMatch[1], 10);
        const nextNumber = lastNumber + 1;
        const prefix = lastClient.name.replace(/\d+$/, "");
        newName = `${prefix}${nextNumber.toString().padStart(3, "0")}`;
      }

      // Extract IP and increment
      const ipMatch = lastClient.ip.match(/^(\d+\.\d+\.\d+\.)(\d+)$/);
      if (ipMatch) {
        const ipBase = ipMatch[1];
        const lastOctet = parseInt(ipMatch[2], 10);
        newIp = `${ipBase}${lastOctet + 1}`;
      }
    }

    setClient({
      name: newName,
      mac: "",
      ip: newIp,
      master: masters.length > 0 ? masters[0].name : "",
      snapshot: "",
      clone: "",
    });
    setIsModalOpen(true);
  }, [clients, masters]);

  // Calculate statistics
  const onlineClients = useMemo(() => clients.filter((c) => c.status === "Online").length, [clients]);
  const offlineClients = clients.length - onlineClients;

  if (loading && clients.length === 0) {
    return (
      <div className="flex flex-col gap-6">
        <Card className="shadow-xl">
          <CardHeader className="flex flex-row items-center justify-between gap-2">
            <div className="flex items-center gap-2 min-w-0">
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">
                <Laptop className="size-4 text-primary" />
              </span>
              <div className="flex min-w-0 flex-col gap-0.5">
                <CardTitle>Client Management</CardTitle>
                <CardDescription>Loading client information...</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <LoadingSkeleton variant="table" count={5} />
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      {/* Page Header with Stats */}
      <Card className="shadow-xl">
        <CardHeader className="flex flex-row items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">
              <Laptop className="size-4 text-primary" />
            </span>
            <div className="flex min-w-0 flex-col gap-0.5">
              <CardTitle>Client Management</CardTitle>
              <CardDescription>
                Manage diskless boot clients and monitor their connection status
              </CardDescription>
            </div>
          </div>
          <CardAction>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setIsScheduledOperationsOpen(true)}
              >
                <Clock data-icon="inline-start" />
                Scheduled Ops
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setIsAuditLogViewerOpen(true)}
              >
                <History data-icon="inline-start" />
                Audit Logs
              </Button>
              <Button
                variant="default"
                size="sm"
                onClick={handleClientFormModalOpen}
              >
                <PlusCircle data-icon="inline-start" />
                Add Client
              </Button>
            </div>
          </CardAction>
        </CardHeader>
        {clients.length > 0 && (
          <CardContent>
            <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
              {/* Total Clients */}
              <Card>
                <CardContent className="flex items-center justify-between">
                  <div>
                    <div className="text-2xl font-semibold text-card-foreground">
                      {clients.length}
                    </div>
                    <div className="text-sm text-muted-foreground">
                      Total Clients
                    </div>
                  </div>
                  <Users className="size-8 text-primary/60" />
                </CardContent>
              </Card>

              {/* Online Clients */}
              <Card>
                <CardContent className="flex items-center justify-between">
                  <div>
                    <div className="text-2xl font-semibold text-card-foreground">
                      {onlineClients}
                    </div>
                    <div className="text-sm text-muted-foreground">Online</div>
                  </div>
                  <Wifi className="size-8 text-primary" />
                </CardContent>
              </Card>

              {/* Offline Clients */}
              <Card>
                <CardContent className="flex items-center justify-between">
                  <div>
                    <div className="text-2xl font-semibold text-card-foreground">
                      {offlineClients}
                    </div>
                    <div className="text-sm text-muted-foreground">Offline</div>
                  </div>
                  <WifiOff className="size-8 text-muted-foreground/60" />
                </CardContent>
              </Card>
            </div>
          </CardContent>
        )}
      </Card>

      {/* Client Table or Empty State */}
      {clients.length === 0 ? (
        <ClientHero handleClientFormModalOpen={handleClientFormModalOpen} />
      ) : (
        <MemoizedClientTable
          handleClientContextMenu={handleClientContextMenu}
        />
      )}
      <MemoizedContextMenu
        isOpen={contextMenu.isOpen}
        xPos={contextMenu.x}
        yPos={contextMenu.y}
        targetClient={contextMenu.client}
        onClose={closeContextMenu}
        actions={contextActions}
      />
      <ClientFormModal
        client={client}
        setClient={setClient}
        masters={masters}
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        refresh={refreshData}
      />
      <RemoteDesktopModal
        client={client}
        isOpen={isRemoteModalOpen}
        onClose={() => setIsRemoteModalOpen(false)}
        onSuccess={refreshData}
      />
      <PowerActionModal
        client={client}
        type={powerAction}
        isOpen={!!powerAction}
        onClose={() => setPowerAction(null)}
        onSuccess={refreshData}
      />
      <AuditLogViewer
        isOpen={isAuditLogViewerOpen}
        onClose={() => setIsAuditLogViewerOpen(false)}
      />
      <ScheduledOperationsList
        isOpen={isScheduledOperationsOpen}
        onClose={() => setIsScheduledOperationsOpen(false)}
      />
    </div>
  );
};

export default ClientManagement;
