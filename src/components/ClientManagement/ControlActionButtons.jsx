import { useState } from "react";
import { Power, RefreshCw, ScreenShare } from "lucide-react";
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
        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Reboot client"
          disabled={!isOnline}
          onClick={() => setPowerAction("reboot")}
        >
          <RefreshCw className="w-4 h-4" />
        </button>

        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Shutdown client"
          disabled={!isOnline}
          onClick={() => setPowerAction("shutdown")}
        >
          <Power className="w-4 h-4" />
        </button>

        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Remote control"
          disabled={!isOnline}
          onClick={() => setRemoteModalOpen(true)}
        >
          <ScreenShare className="w-4 h-4" />
        </button>
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
