import { useClientActions } from "@/hooks/useClientActions";
import {
  Card,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/ui";
import { LoadingSkeleton } from "@/components/ui/LoadingSkeleton";
import { ContextMenu } from "../ui/ContextMenu";
import { PlusCircle, Users, Wifi, WifiOff, History, Clock } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "../../store/useAppStore";
import { isOnlineStatus } from "@/lib/clientStatus";
import ClientFormModal from "./ClientFormModal";
import { getNextClientIdentity } from "./clientDefaults";
import ClientTable from "./ClientTable";
import ClientHero from "./ClientHero";
import RemoteDesktopModal from "./RemoteDesktopModal";
import PowerActionModal from "./PowerActionModal";
import AuditLogViewer from "./AuditLogViewer";
import ScheduledOperationsList from "./ScheduledOperationsList";
import LegacySuperExitDialog from "./LegacySuperExitDialog";
import { getSuperStatus } from "@/api/modules/clients";
import { getPendingClientRecoveries } from "@/api/modules/reconciliation";
import { useAuth } from "@/contexts/auth";

const MemoizedClientTable = memo(ClientTable);
const MemoizedContextMenu = memo(ContextMenu);

const ClientManagement = () => {
  const { user } = useAuth();
  const {
    clients,
    clientsError,
    fetchClients,
    fetchImages,
    masters,
    loading,
  } = useAppStore(
    useShallow((state) => ({
      clients: state.clients,
      clientsError: state.clientsError,
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
  const [legacySuperClient, setLegacySuperClient] = useState(null);
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
  const contextMenuRequestId = useRef(0);
  const [superData, setSuperData] = useState(null);

  useEffect(() => {
    const target = contextMenu.client;
    if (!contextMenu.isOpen || !target || user?.role !== "admin") return;
    let current = true;
    Promise.all([getSuperStatus(target.id), getPendingClientRecoveries()])
      .then(([status, recoveryResponse]) => {
        if (!current) return;
        const storageRecovery = (recoveryResponse.recoveries || []).some(
          (recovery) => recovery.client_id === target.id && recovery.kind !== "boot_failure",
        );
        setSuperData({ requestId: contextMenu.requestId, status: { ...status, storage_recovery: storageRecovery } });
      })
      .catch(() => {
        if (!current) return;
        setSuperData({ requestId: contextMenu.requestId, status: null, error: true });
      });
    return () => { current = false; };
  }, [contextMenu.client, contextMenu.isOpen, contextMenu.requestId, user?.role]);

  const currentSuperData = superData?.requestId === contextMenu.requestId ? superData : null;

  const handleClientContextMenu = useCallback((event, client) => {
    event.preventDefault();
    setContextMenu({
      isOpen: true,
      x: event.clientX,
      y: event.clientY,
      client: client,
      requestId: ++contextMenuRequestId.current,
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
    const { name: newName, ip: newIp } = getNextClientIdentity(clients);

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
  const onlineClients = useMemo(() => clients.filter((c) => isOnlineStatus(c.status)).length, [clients]);
  const offlineClients = clients.length - onlineClients;

  if (loading && clients.length === 0) {
    return (
      <div className="flex flex-col gap-4">
        <div className="space-y-1">
          <Skeleton className="h-7 w-48" />
          <Skeleton className="h-4 w-96 max-w-full" />
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} className="h-24 rounded-xl" />
          ))}
        </div>
        <Card>
          <CardContent>
            <LoadingSkeleton variant="table" count={5} />
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Client Management"
        description="Manage diskless boot clients and monitor their connection status"
        actions={
          <div className="flex w-full flex-wrap gap-2 sm:w-auto [&>button]:flex-1 sm:[&>button]:flex-none">
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
        }
      />
      {clientsError && (
        <div
          role="alert"
          className="flex items-center justify-between gap-4 rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm"
        >
          <span>Failed to load clients: {clientsError}</span>
          <Button variant="outline" onClick={() => fetchClients()}>
            Retry
          </Button>
        </div>
      )}
      {clients.length > 0 && (
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
      )}

      {/* Client Table or Empty State */}
      {clients.length === 0 ? (
        clientsError ? null : <ClientHero handleClientFormModalOpen={handleClientFormModalOpen} />
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
        onExitLegacySuper={setLegacySuperClient}
        superStatus={currentSuperData?.status}
        superLoading={!currentSuperData}
        superError={currentSuperData?.error}
        targetImage={masters.find((image) => image.name === contextMenu.client?.master || image.id === contextMenu.client?.master)}
      />
      {legacySuperClient && (
        <LegacySuperExitDialog
          client={clients.find((item) => item.id === legacySuperClient.id) || legacySuperClient}
          masters={masters}
          isOpen
          onClose={() => setLegacySuperClient(null)}
          refresh={refreshData}
        />
      )}
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
