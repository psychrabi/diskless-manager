import React, { useState } from "react";
import { Power, RefreshCw, ScreenShare } from "lucide-react";
import { useToastStore } from "@/store/useToastStore";
import * as api from "@/api/commands";
import ShutdownModal from "./ShutdownModal";
import RebootModal from "./RebootModal";
import OperationConfirmDialog from "./OperationConfirmDialog";

const ControlActionButtons = ({ client, onActionComplete }) => {
  const { success, error: showError } = useToastStore();
  const [loading, setLoading] = useState(null);
  const [shutdownModalOpen, setShutdownModalOpen] = useState(false);
  const [rebootModalOpen, setRebootModalOpen] = useState(false);
  const [remoteConfirmOpen, setRemoteConfirmOpen] = useState(false);
  const [remoteLoading, setRemoteLoading] = useState(false);

  const isOnline = client?.status === "Online";

  const handleRemoteDesktop = async () => {
    if (!isOnline) {
      showError("Control Operations", "Client must be online for remote access.");
      return;
    }

    setRemoteLoading(true);
    try {
      const response = await api.remoteDesktopClient(client.id);
      success(
        "Control Operations",
        response?.message || "Remote desktop connection initiated"
      );
      setRemoteConfirmOpen(false);
      if (onActionComplete) {
        onActionComplete();
      }
    } catch (error) {
      showError(
        "Control Operations",
        `Failed to connect: ${error.message || String(error)}`
      );
    } finally {
      setRemoteLoading(false);
    }
  };

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
          disabled={!isOnline || loading !== null}
          onClick={() => setRebootModalOpen(true)}
        >
          {loading === "reboot" ? (
            <span className="loading loading-spinner loading-xs"></span>
          ) : (
            <RefreshCw className="w-4 h-4" />
          )}
        </button>

        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Shutdown client"
          disabled={!isOnline || loading !== null}
          onClick={() => setShutdownModalOpen(true)}
        >
          {loading === "shutdown" ? (
            <span className="loading loading-spinner loading-xs"></span>
          ) : (
            <Power className="w-4 h-4" />
          )}
        </button>

        <button
          className="btn btn-sm btn-ghost btn-circle"
          title="Remote control"
          disabled={!isOnline || loading !== null}
          onClick={() => setRemoteConfirmOpen(true)}
        >
          {loading === "remote" ? (
            <span className="loading loading-spinner loading-xs"></span>
          ) : (
            <ScreenShare className="w-4 h-4" />
          )}
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

      <OperationConfirmDialog
        isOpen={remoteConfirmOpen}
        onClose={() => setRemoteConfirmOpen(false)}
        onConfirm={handleRemoteDesktop}
        title="Remote Desktop Access"
        description={`Connect to "${client?.name}" via remote desktop?`}
        clientName={client?.name}
        isLoading={remoteLoading}
        variant="warning"
      />
    </>
  );
};

export default ControlActionButtons;
