import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { useCallback } from "react";
import * as api from "../api/commands";

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
        if (response && response.message)
          success("Client Management", response.message);
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

  // Wrapper functions to match original interface
  return {
    edit: (client) => handleAction(client, "edit"),

    reboot: (client) =>
      handleAction(
        client,
        "reboot",
        "Reboot Client",
        `Are you sure you want to reboot client "${client.name}" ? `,
        "Reboot Client",
        () => api.updateClient(client.id, { action: "reboot" }),
        "Client Rebooted",
        "Client reboot cancelled."
      ),

    shutdown: (client) =>
      handleAction(
        client,
        "shutdown",
        "Shutdown Client",
        `Are you sure you want to shutdown client "${client.name}" ? `,
        "Shutdown Client",
        () => api.updateClient(client.id, { action: "shutdown" }),
        "Client Shutdown",
        "Client shutdown cancelled."
      ),

    wake: (client) =>
      handleAction(
        client,
        "wake",
        "Wake Client",
        `Are you sure you want to wake client "${client.name}" ? `,
        "Wake Client",
        () => api.updateClient(client.id, { action: "wake" }),
        "Client Woken",
        "Client wake up cancelled."
      ),

    remote: (client) =>
      handleAction(
        client,
        "remote",
        "Remote Client",
        `Are you sure you want to remote client "${client.name}" ? `,
        "Remote Client",
        () => api.updateClient(client.id, { action: "remote" }),
        "Client Remotely Connected",
        "Client remote connection cancelled."
      ),

    reset: (client) =>
      handleAction(
        client,
        "reset",
        "Reset client writeback",
        `Are you sure you want to reset client "${client.name}" ? This will destroy their ZFS clone and remove configurations.`,
        "Reset Client",
        () => api.updateClient(client.id, { action: "reset" }),
        "Client Reset successfully",
        "Client reset cancelled."
      ),

    resetToClean: async (client) => {
      if (!client) return;

      // Check if client is in non-persistent mode
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
        `This will delete the writeback for "${client.name}" and recreate it from the snapshot.All changes will be lost.Continue ? `,
        "Reset to Clean",
        () => api.updateClient(client.id, { action: "reset_clean" }),
        "Client Reset to Clean successfully",
        "Reset to clean cancelled."
      );
    },

    delete: async (client) => {
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
        await api.deleteClient(client.id);
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

    enableSuper: (client) =>
      handleAction(
        client,
        "enableSuper",
        "Enable Super Client",
        `Client "${client.name}" will boot directly from master image.This skips clone / writeback.Continue ? `,
        "Enable Super",
        () => api.updateClient(client.id, { action: "super", make_super: true }),
        "Client Enabled Super successfully",
        "Enable Super cancelled."
      ),

    disableSuper: (client) => {
      if (client.mode !== "super") {
        showError("Client Management", "Client is not in Super mode.");
        return;
      }
      handleAction(
        client,
        "disableSuper",
        "Disable Super Client",
        `This will point ${client.name} back to its writeback clone.Continue ? `,
        "Disable Super",
        () => api.updateClient(client.id, { action: "super", make_super: false }),
        "Client Disabled Super successfully",
        "Disable Super cancelled."
      );
    },

    saveSuper: async (client) => {
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
        const response = await api.createSnapshot(client.master, snapshotName);
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
  };
};
