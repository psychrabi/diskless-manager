import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { useCallback } from "react";
import { updateClient, deleteClient, enterSuper, saveSuper as saveSuperSession, discardSuper as discardSuperSession } from "../api/modules/clients";
import { requestInventoryBoot } from "../api/modules/networkDrivers";
import { isOnlineStatus } from "../lib/clientStatus";

export const useClientActions = (
  fetchData,
  closeContextMenu,
  setClient,
  setIsModalOpen,
  openRemoteDesktop,
  openPowerAction
) => {
  const { success, error: showError, info } = useToastStore();
  const confirm = useConfirm();

  const handleAction = useCallback(
    async (
      client,
      action,
      confirmTitle,
      confirmDesc,
      confirmText,
      apiCall,
      successMsg,
      cancelMsg
    ) => {
      if (!client) return;

      // Pre-checks
      if (
        ["reboot", "shutdown", "remote"].includes(action) &&
        !isOnlineStatus(client.status)
      ) {
        showError("Client Management", `Client must be online to ${action}.`);
        return;
      }
      if (
        [
          "edit",
          "wake",
          "reset",
          "delete",
          "enableSuper",
          "disableSuper",
          "saveSuper",
        ].includes(action) &&
        isOnlineStatus(client.status)
      ) {
        showError(
          "Client Management",
          `Client must be offline to ${
            action === "edit" ? "make changes" : action
          }.`
        );
        return;
      }

      if (action === "edit") {
        setClient(client);
        setIsModalOpen(true);
        closeContextMenu();
        return;
      }

      // Confirmation
      const ok = await confirm({
        title: confirmTitle,
        description: confirmDesc,
        confirmText: confirmText,
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "2xl",
      });

      if (!ok) {
        if (cancelMsg) info(cancelMsg);
        closeContextMenu();
        return;
      }

      // Execution
      try {
        const response = await apiCall();
        if (response && response.message) {
          success("Client Management", response.message);
        } else if (successMsg) {
          success("Client Management", successMsg);
        }
        if (fetchData) fetchData();
        if (closeContextMenu) closeContextMenu();
      } catch (error) {
        showError(
          "Client Management",
          `Failed to execute ${action}: ${error.message || String(error)}`
        );
      }
    },
    [confirm, showError, closeContextMenu, fetchData, setClient, setIsModalOpen, success, info]
  );

  const handleEdit = useCallback((client) => handleAction(client, "edit"), [handleAction]);

  const handleReboot = useCallback(
    (client) => {
      if (!client || !isOnlineStatus(client.status)) {
        showError("Client Management", "Client must be online to reboot.");
        return;
      }
      if (!openPowerAction) {
        showError("Client Management", "Power action modal is not available.");
        return;
      }
      closeContextMenu();
      setClient(client);
      openPowerAction("reboot");
    },
    [closeContextMenu, setClient, showError, openPowerAction]
  );

  const handleShutdown = useCallback(
    (client) => {
      if (!client || !isOnlineStatus(client.status)) {
        showError("Client Management", "Client must be online to shutdown.");
        return;
      }
      if (!openPowerAction) {
        showError("Client Management", "Power action modal is not available.");
        return;
      }
      closeContextMenu();
      setClient(client);
      openPowerAction("shutdown");
    },
    [closeContextMenu, setClient, showError, openPowerAction]
  );

  const handleWake = useCallback(
    (client) =>
      handleAction(
        client,
        "wake",
        "Wake Client",
        `Are you sure you want to wake client "${client.name}" ? `,
        "Wake Client",
        () => updateClient(client.id, { action: "wake" }),
        "Client Woken",
        "Client wake up cancelled."
      ),
    [handleAction]
  );

  const handleRemote = useCallback(
    (client) => {
      if (!client) return;

      if (!isOnlineStatus(client.status)) {
        showError("Client Management", "Client must be online to remote.");
        return;
      }

      closeContextMenu();
      setClient(client);

      if (openRemoteDesktop) {
        openRemoteDesktop(client);
      }
    },
    [closeContextMenu, setClient, showError, openRemoteDesktop]
  );

  const handleReset = useCallback(
    (client) =>
      handleAction(
        client,
        "reset",
        "Reset client writeback",
        `Are you sure you want to reset client "${client.name}" ? This will destroy their ZFS clone and remove configurations.`,
        "Reset Client",
        () => updateClient(client.id, { action: "reset" }),
        `Successfully reset writeback for client '${client.name}'`,
        "Client reset cancelled."
      ),
    [handleAction]
  );

  const handleResetToClean = useCallback(
    async (client) => {
      if (!client) return;

      if (client.keep_writeback !== false) {
        showError(
          "Client Management",
          "Client is in persistent mode. Only non-persistent clients can be reset to clean state."
        );
        return;
      }

      handleAction(
        client,
        "resetToClean",
        "Reset to Clean State",
        `This will delete the writeback for "${client.name}" and recreate it from the snapshot. All changes will be lost. Continue?`,
        "Reset to Clean",
        () => updateClient(client.id, { action: "reset_clean" }),
        `Successfully reset client '${client.name}' to clean state`,
        "Reset to clean cancelled."
      );
    },
    [handleAction, showError]
  );

  const handleInventoryBoot = useCallback(
    (client) =>
      handleAction(
        client,
        'inventoryBoot',
        'Request Inventory Boot',
        `"${client.name}" will boot WinPE on its next PXE boot to record its network adapter IDs, then return to the normal boot menu. Continue?`,
        'Request Inventory Boot',
        () => requestInventoryBoot(client.id),
        `Inventory boot requested for '${client.name}'. Reboot the machine to collect its NIC inventory.`,
        'Inventory boot request cancelled.'
      ),
    [handleAction]
  );

  const handleDelete = useCallback(
    async (client) => {
      if (!client) return;

      const ok = await confirm({
        title: "Delete Client",
        description: `Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`,
        confirmText: "Delete Client",
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "2xl",
      });

      if (!ok) {
        info("Client deletion cancelled.");
        closeContextMenu();
        return;
      }

      try {
        await deleteClient(client.id);
        success("Client Management", "Client Deleted successfully");
        if (fetchData) fetchData();
        if (closeContextMenu) closeContextMenu();
      } catch (e) {
        showError(
          "Client Management",
          `Failed to execute delete: ${e.message || String(e)}`
        );
      }
    },
    [confirm, showError, closeContextMenu, fetchData, success, info]
  );

  const handleEnableSuper = useCallback(async (client, verificationOnly = false, status) => {
    if (!client) return;
    if (isOnlineStatus(client.status) || client.mutation_state || client.recovery_required || status?.storage_recovery) {
      showError("Client Management", "Client must be offline with no pending mutation or recovery to start Super mode.");
      return;
    }
    const ok = await confirm({
      title: verificationOnly ? "Start Verification Session" : "Start Super Session",
      description: verificationOnly
        ? `Start a disposable installer boot test for ${client.name}. Its working changes will be discarded and cannot be saved.`
        : `Start a clone-backed installer session for ${client.name}. Save publishes changes as a new image revision; Discard restores the current assignment.`,
      confirmText: verificationOnly ? "Start Test Session" : "Start Super Session",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (!ok) return;
    try {
      await enterSuper(client.id, { verification_only: verificationOnly, confirmed: verificationOnly });
      success("Client Management", verificationOnly ? "Disposable verification session started." : "Super session started.");
      fetchData?.();
      closeContextMenu?.();
    } catch (error) {
      showError("Client Management", `Failed to start Super session: ${error.message || String(error)}`);
    }
  }, [confirm, showError, success, fetchData, closeContextMenu]);

  const handleSaveSuper = useCallback(async (client, status) => {
    const session = status?.session;
    if (!client) return;
    if (session?.verification_only) {
      showError("Client Management", "Verification-only sessions cannot be saved.");
      return;
    }
    if (isOnlineStatus(client.status) || client.mutation_state || client.recovery_required || status?.storage_recovery || session?.state !== "active") {
      showError("Client Management", "Save is available only for an offline active Super session with no pending mutation or recovery.");
      return;
    }
    const ok = await confirm({
      title: "Save Super Changes",
      description: `Publish ${client.name}'s installer changes as a new image revision and return the client to normal mode?`,
      confirmText: "Save Changes",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (!ok) return;
    try {
      await saveSuperSession(client.id);
      success("Client Management", "Super changes saved as a new image revision.");
      fetchData?.();
      closeContextMenu?.();
    } catch (error) {
      showError("Client Management", `Failed to save Super changes: ${error.message || String(error)}`);
    }
  }, [confirm, showError, success, fetchData, closeContextMenu]);

  const handleDiscardSuper = useCallback(async (client, status) => {
    const session = status?.session;
    if (!client) return;
    if (isOnlineStatus(client.status) || client.mutation_state || client.recovery_required || status?.storage_recovery || session?.state !== "active") {
      showError("Client Management", "Discard is available only for an offline active Super session with no pending mutation or recovery.");
      return;
    }
    const ok = await confirm({
      title: "Discard Super Changes",
      description: `Permanently remove ${client.name}'s Super-session changes and restore its prior image assignment?`,
      confirmText: "Discard Changes",
      cancelText: "Cancel",
      confirmVariant: "destructive",
      size: "2xl",
    });
    if (!ok) return;
    try {
      await discardSuperSession(client.id);
      success("Client Management", "Super changes discarded; the prior assignment was restored.");
      fetchData?.();
      closeContextMenu?.();
    } catch (error) {
      showError("Client Management", `Failed to discard Super changes: ${error.message || String(error)}`);
    }
  }, [confirm, showError, success, fetchData, closeContextMenu]);

  return {
    edit: handleEdit,
    reboot: handleReboot,
    shutdown: handleShutdown,
    wake: handleWake,
    remote: handleRemote,
    reset: handleReset,
    resetToClean: handleResetToClean,
    inventoryBoot: handleInventoryBoot,
    delete: handleDelete,
    startSuper: handleEnableSuper,
    discardSuper: handleDiscardSuper,
    saveSuper: handleSaveSuper,
  };
};
