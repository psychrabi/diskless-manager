import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { useCallback } from "react";
import { updateClient, deleteClient } from "../api/modules/clients";
import { createSnapshot } from "../api/modules/images";

export const useClientActions = (
  fetchData,
  closeContextMenu,
  setClient,
  setIsModalOpen
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
        client.status !== "Online"
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
        client.status === "Online"
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
    (client) =>
      handleAction(
        client,
        "reboot",
        "Reboot Client",
        `Are you sure you want to reboot client "${client.name}" ? `,
        "Reboot Client",
        () => updateClient(client.id, { action: "reboot" }),
        "Client Rebooted",
        "Client reboot cancelled."
      ),
    [handleAction]
  );

  const handleShutdown = useCallback(
    (client) =>
      handleAction(
        client,
        "shutdown",
        "Shutdown Client",
        `Are you sure you want to shutdown client "${client.name}" ? `,
        "Shutdown Client",
        () => updateClient(client.id, { action: "shutdown" }),
        "Client Shutdown",
        "Client shutdown cancelled."
      ),
    [handleAction]
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
    (client) =>
      handleAction(
        client,
        "remote",
        "Remote Client",
        `Are you sure you want to remote client "${client.name}" ? `,
        "Remote Client",
        () => updateClient(client.id, { action: "remote" }),
        "Client Remotely Connected",
        "Client remote connection cancelled."
      ),
    [handleAction]
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

  const handleEnableSuper = useCallback(
    (client) =>
      handleAction(
        client,
        "enableSuper",
        "Enable Super Client",
        `Client "${client.name}" will boot directly from master image. This skips clone / writeback. Continue?`,
        "Enable Super",
        () => updateClient(client.id, { action: "super", make_super: true }),
        "Client Enabled Super successfully",
        "Enable Super cancelled."
      ),
    [handleAction]
  );

  const handleDisableSuper = useCallback(
    (client) => {
      if (client.mode !== "super") {
        showError("Client Management", "Client is not in Super mode.");
        return;
      }
      handleAction(
        client,
        "disableSuper",
        "Disable Super Client",
        `This will point ${client.name} back to its writeback clone. Continue?`,
        "Disable Super",
        () => updateClient(client.id, { action: "super", make_super: false }),
        "Client Disabled Super successfully",
        "Disable Super cancelled."
      );
    },
    [handleAction, showError]
  );

  const handleSaveSuper = useCallback(
    async (client) => {
      if (!client) return;
      if (client.mode !== "super") {
        showError("Client Management", "Client is not in Super mode.");
        return;
      }
      if (client.status !== "Offline") {
        showError("Client Management", "Client must be offline to save Super.");
        return;
      }

      const defaultSuffix = `${client.name}-super-${Date.now()}`;
      const suffix = await confirm({
        title: "Save Super Client",
        description: `This will save the current state of ${client.name} to a snapshot. Please enter a name for the new snapshot:`,
        confirmText: "Save Snapshot",
        confirmVariant: "success",
        showInput: true,
        inputLabel: "Snapshot Name Suffix",
        inputPlaceholder: "e.g. updated-drivers",
        defaultValue: defaultSuffix,
        size: "lg",
      });

      if (!suffix) {
        info("Save Super cancelled.");
        return;
      }

      if (typeof suffix === "string" && !/^[-\w\s]+$/.test(suffix)) {
        showError(
          "Client Management",
          "Invalid snapshot name suffix. Use alphanumeric characters, spaces, dashes or underscores."
        );
        return;
      }

      const snapshotName = `${client.master}@${suffix
        .trim()
        .replace(/\s+/g, "-")}`;

      try {
        const response = await createSnapshot(client.master, snapshotName);
        if (response.message) success("Client Management", response.message);
        fetchData();
        closeContextMenu();
      } catch (error) {
        showError(
          "Client Management",
          `Failed to save super: ${error.message || String(error)}`
        );
      }
    },
    [confirm, showError, info, success, fetchData, closeContextMenu]
  );

  return {
    edit: handleEdit,
    reboot: handleReboot,
    shutdown: handleShutdown,
    wake: handleWake,
    remote: handleRemote,
    reset: handleReset,
    resetToClean: handleResetToClean,
    delete: handleDelete,
    enableSuper: handleEnableSuper,
    disableSuper: handleDisableSuper,
    saveSuper: handleSaveSuper,
  };
};
