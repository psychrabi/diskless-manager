import { useState } from "react";
import { Power, RefreshCw, ScreenShare } from "lucide-react";
import { Button } from "@/components/ui/button";
import PowerActionModal from "./PowerActionModal";
import RemoteDesktopModal from "./RemoteDesktopModal";

const ControlActionButtons = ({ client, onActionComplete }) => {
  const [powerAction, setPowerAction] = useState(null);
  const [remoteModalOpen, setRemoteModalOpen] = useState(false);

  const isOnline = client?.status === "Online";

  const handleActionComplete = () => {
    if (onActionComplete) {
      onActionComplete();
    }
  };

  return (
    <>
      <div className="flex gap-1">
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
          title="Shutdown client"
          disabled={!isOnline}
          onClick={() => setPowerAction("shutdown")}
        >
          <Power />
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
