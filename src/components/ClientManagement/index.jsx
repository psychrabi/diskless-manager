import { useClientActions } from "@/hooks/useClientActions";
import { StatusBadge, LoadingSkeleton } from "@/components/ui";
import { Laptop, PlusCircle, Users, Wifi, WifiOff } from "lucide-react";
import { memo, useCallback, useState } from "react";
import { Link } from "react-router-dom";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card } from "@/components/ui";
import { ContextMenu } from "../ui/ContextMenu";
import ClientFormModal from "./ClientFormModal";
import ClientTable from "./ClientTable";

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
  const onlineClients = clients.filter(c => c.status === "online").length;
  const offlineClients = clients.length - onlineClients;

  if (loading && clients.length === 0) {
    return (
      <div className="space-y-6">
        <Card
          title="Client Management"
          subtitle="Loading client information..."
          icon={Laptop}
          variant="elevated"
        >
          <LoadingSkeleton variant="table" count={5} />
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page Header with Stats */}
      <Card
        title="Client Management"
        subtitle="Manage diskless boot clients and monitor their connection status"
        icon={Laptop}
        variant="elevated"
        actions={
          <Button
            variant="primary"
            onClick={handleClientFormModalOpen}
            icon={PlusCircle}
            size="sm"
          >
            Add Client
          </Button>
        }
      >
        {clients.length > 0 && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {/* Total Clients */}
            <div className="card-professional bg-gradient-to-br from-primary/10 to-primary/5 border-primary/20">
              <div className="card-body-professional py-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-heading-sm font-semibold text-base-content">
                      {clients.length}
                    </div>
                    <div className="text-body-sm text-base-content/60">
                      Total Clients
                    </div>
                  </div>
                  <Users className="h-8 w-8 text-primary/60" />
                </div>
              </div>
            </div>

            {/* Online Clients */}
            <div className="card-professional bg-gradient-to-br from-success/10 to-success/5 border-success/20">
              <div className="card-body-professional py-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-heading-sm font-semibold text-base-content">
                      {onlineClients}
                    </div>
                    <div className="text-body-sm text-base-content/60">
                      Online
                    </div>
                  </div>
                  <Wifi className="h-8 w-8 text-success/60" />
                </div>
              </div>
            </div>

            {/* Offline Clients */}
            <div className="card-professional bg-gradient-to-br from-base-300/50 to-base-200/30 border-base-300">
              <div className="card-body-professional py-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-heading-sm font-semibold text-base-content">
                      {offlineClients}
                    </div>
                    <div className="text-body-sm text-base-content/60">
                      Offline
                    </div>
                  </div>
                  <WifiOff className="h-8 w-8 text-base-content/40" />
                </div>
              </div>
            </div>
          </div>
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
    </div>
  );
};

export default ClientManagement;