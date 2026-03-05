import React, { useState } from "react";
import { Power, RefreshCw, ScreenShare } from "lucide-react";
import ShutdownModal from "./ShutdownModal";
import RebootModal from "./RebootModal";
import RemoteDesktopModal from "./RemoteDesktopModal";

const ControlActionButtons = ({ client, onActionComplete }) => {
  const [shutdownModalOpen, setShutdownModalOpen] = useState(false);
  const [rebootModalOpen, setRebootModalOpen] = useState(false);
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
          onClick={() => setRebootModalOpen(true)}
        >
          <RefreshCw className="w-4 h-4" />
        </button>

        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Shutdown client"
          disabled={!isOnline}
          onClick={() => setShutdownModalOpen(true)}
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
      <ShutdownModal
        client={client}
        isOpen={shutdownModalOpen}
        onClose={() => setShutdownModalOpen(false)}
        onSuccess={handleActionComplete}
      />

      <RebootModal
        client={client}
        isOpen={rebootModalOpen}
        onClose={() => setRebootModalOpen(false)}
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
