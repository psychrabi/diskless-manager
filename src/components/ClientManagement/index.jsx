import { useClientActions } from "@/hooks/useClientActions";
import { PlusCircle, Users } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card } from "../ui";
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
    startClientStatusPolling,
    stopClientStatusPolling,
  } = useAppStore(
    useShallow((state) => ({
      clients: state.clients,
      fetchClients: state.fetchClients,
      fetchImages: state.fetchImages,
      masters: state.masters,
      startClientStatusPolling: state.startClientStatusPolling,
      stopClientStatusPolling: state.stopClientStatusPolling,
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
        a.name.localeCompare(b.name, undefined, {
          numeric: true,
          sensitivity: "base",
        }),
      );
      const lastClient = sortedClients[sortedClients.length - 1];

      // Increment name (e.g., pc002 -> pc003)
      const nameMatch = lastClient.name.match(/^(.*?)(\d+)$/);
      if (nameMatch) {
        const prefix = nameMatch[1];
        const numberPart = nameMatch[2];
        const num = parseInt(numberPart, 10) + 1;
        newName = `${prefix}${num.toString().padStart(numberPart.length, "0")}`;
      }

      // Increment IP last octet
      // Find the highest IP to avoid collisions
      const sortedIps = [...clients]
        .map((c) => c.ip)
        .filter((ip) => ip.startsWith("192.168.1.")) // Assuming standard subnet
        .map((ip) => parseInt(ip.split(".")[3], 10))
        .sort((a, b) => a - b);

      if (sortedIps.length > 0) {
        const lastOctet = sortedIps[sortedIps.length - 1];
        if (lastOctet < 254) {
          newIp = `192.168.1.${lastOctet + 1}`;
        }
      }
    }

    setClient({
      name: newName,
      mac: "",
      ip: newIp,
      master: masters[0]?.name || "",
      snapshot: "",
      clone: "",
    });
    setIsModalOpen(true);
  }, [clients, masters]);

  useEffect(() => {
    // Start polling when this page mounts; stop when it unmounts
    startClientStatusPolling();
    return () => stopClientStatusPolling();
  }, [startClientStatusPolling, stopClientStatusPolling]);

  return (
    <Card
      title="Client Management"
      icon={Users}
      className="bg-base-300"
      actions={
        <Button
          variant="primary"
          onClick={handleClientFormModalOpen}
          icon={PlusCircle}
          disabled={masters.length === 0}
        >
          Add Client{" "}
          {masters.length === 0 && (
            <span className="text-xs text-error ml-2 self-center">
              (Requires Master Image)
            </span>
          )}
        </Button>
      }
    >
      <div className="min-h-[calc(100vh-15rem)]">
        <MemoizedClientTable
          handleClientContextMenu={handleClientContextMenu}
        />
        <MemoizedContextMenu
          isOpen={contextMenu.isOpen}
          xPos={contextMenu.x}
          yPos={contextMenu.y}
          targetClient={contextMenu.client}
          onClose={closeContextMenu}
          actions={contextActions}
        />
      </div>
      <ClientFormModal
        client={client}
        setClient={setClient}
        masters={masters}
        isOpen={isModalOpen}
        setIsOpen={setIsModalOpen}
        refresh={refreshData}
      />
    </Card>
  );
};

export default ClientManagement;
