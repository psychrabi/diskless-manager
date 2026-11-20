import { useState } from "react";
import { ChevronDown, Play, Power, RefreshCw, ScreenShare } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { updateClient } from "@/api/modules/clients";
import { useToastStore } from "@/store/useToastStore";
import PowerActionModal from "./PowerActionModal";
import RemoteDesktopModal from "./RemoteDesktopModal";
import { isOnlineStatus } from "@/lib/clientStatus";

const ControlActionButtons = ({ client, onActionComplete }) => {
  const [powerAction, setPowerAction] = useState(null);
  const [remoteModalOpen, setRemoteModalOpen] = useState(false);
  const [waking, setWaking] = useState(false);
  const { success, error } = useToastStore();

  const isOnline = isOnlineStatus(client?.status);

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
      <DropdownMenu>
        <DropdownMenuTrigger
          render={<Button variant="outline" size="sm" className="h-11 xl:h-8" />}
          aria-label={`Controls for ${client.name}`}
        >
          Controls <ChevronDown data-icon="inline-end" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {isOnline ? (
            <>
              <DropdownMenuItem onClick={() => setRemoteModalOpen(true)}><ScreenShare /> Remote control</DropdownMenuItem>
              <DropdownMenuItem onClick={() => setPowerAction("reboot")}><RefreshCw /> Reboot</DropdownMenuItem>
              <DropdownMenuItem onClick={() => setPowerAction("shutdown")}><Power /> Shut down</DropdownMenuItem>
            </>
          ) : (
            <DropdownMenuItem disabled={waking} onClick={handleWake}><Play /> Power on</DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

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
