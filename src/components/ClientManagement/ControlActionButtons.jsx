import { useState } from "react";
import { Play, Power, RefreshCw, ScreenShare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { updateClient } from "@/api/modules/clients";
import { useToastStore } from "@/store/useToastStore";
import PowerActionModal from "./PowerActionModal";
import RemoteDesktopModal from "./RemoteDesktopModal";

const ControlActionButtons = ({ client, onActionComplete }) => {
  const [powerAction, setPowerAction] = useState(null);
  const [remoteModalOpen, setRemoteModalOpen] = useState(false);
  const [waking, setWaking] = useState(false);
  const { success, error } = useToastStore();

  const isOnline = client?.status === "Online";

  const handleActionComplete = () => {
    if (onActionComplete) {
      onActionComplete();
    }
  };

  const handleWake = async () => {
    if (!client?.id || waking) return;
    setWaking(true);
    try {
      const response = await updateClient(client.id, { action: "wake" });
      success("Client Management", response?.message || `Wake signal sent to ${client.name}.`);
      handleActionComplete();
    } catch (e) {
      error("Client Management", e?.message || String(e));
    } finally {
      setWaking(false);
    }
  };

  return (
    <>
      <div className="flex justify-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          title="Reboot client"
          disabled={!isOnline}
          onClick={() => setPowerAction("reboot")}
        >
          <RefreshCw />
        </Button>

        <Button
          variant="ghost"
          size="icon"
          title={isOnline ? "Shutdown client" : "Power on client"}
          disabled={waking}
          onClick={() => (isOnline ? setPowerAction("shutdown") : handleWake())}
        >
          {isOnline ? <Power /> : <Play />}
        </Button>

        <Button
          variant="ghost"
          size="icon"
          title="Remote control"
          disabled={!isOnline}
          onClick={() => setRemoteModalOpen(true)}
        >
          <ScreenShare />
        </Button>
      </div>

      {/* Modals */}
      <PowerActionModal
        client={client}
        type={powerAction}
        isOpen={!!powerAction}
        onClose={() => setPowerAction(null)}
        onSuccess={handleActionComplete}
      />

      <RemoteDesktopModal
        client={client}
        isOpen={remoteModalOpen}
        onClose={() => setRemoteModalOpen(false)}
        onSuccess={handleActionComplete}
      />
    </>
  );
};

export default ControlActionButtons;
