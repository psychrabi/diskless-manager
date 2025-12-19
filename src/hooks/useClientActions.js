import { useConfirm } from "@/contexts/confirmDialog";
import { useNotification } from "@/contexts/notification";
import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";

export const useClientActions = (
  fetchData,
  closeContextMenu,
  setClient,
  setIsModalOpen,
) => {
  const { showNotification } = useNotification();
  const confirm = useConfirm();

  const handleAction = useCallback(
    async (
      client,
      action,
      confirmTitle,
      confirmDesc,
      confirmText,
      invokeCmd,
      invokeArgs,
      successMsg,
      cancelMsg,
    ) => {
      if (!client) return;

      // Pre-checks
      if (
        ["reboot", "shutdown", "remote"].includes(action) &&
        client.status !== "Online"
      ) {
        showNotification(`Client must be online to ${action}.`, "error");
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
        // Edit is special case in original code: "Client must be offine to make changes."
        // Wake is special case: "Client must be offline to wake"
        // Others: "Client must be offline to..."
        // So generally, offline required for these.
        showNotification(
          `Client must be offline to ${
            action === "edit" ? "make changes" : action
          }.`,
          "error",
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
        if (cancelMsg) showNotification(cancelMsg, "info");
        closeContextMenu();
        return;
      }

      // Execution
      const token = localStorage.getItem("authToken") || "";
      try {
        const response = await invoke(invokeCmd, { token, ...invokeArgs });
        if (response && response.message)
          showNotification(response.message, "success");
        if (fetchData) fetchData();
        if (closeContextMenu) closeContextMenu();
      } catch (error) {
        showNotification(
          `Failed to execute ${action}: ${error.message || String(error)}`,
          "error",
        );
      }
    },
    [
      confirm,
      showNotification,
      closeContextMenu,
      fetchData,
      setClient,
      setIsModalOpen,
    ],
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
        "control_client",
        { clientId: client.id, req: { action: "reboot" } },
        "Client Rebooted",
        "Client reboot cancelled.",
      ),

    shutdown: (client) =>
      handleAction(
        client,
        "shutdown",
        "Shutdown Client",
        `Are you sure you want to shutdown client "${client.name}" ? `,
        "Shutdown Client",
        "control_client",
        { clientId: client.id, req: { action: "shutdown" } },
        "Client Shutdown",
        "Client shutdown cancelled.",
      ),

    wake: (client) =>
      handleAction(
        client,
        "wake",
        "Wake Client",
        `Are you sure you want to wake client "${client.name}" ? `,
        "Wake Client",
        "control_client",
        { clientId: client.id, req: { action: "wake" } },
        "Client Woken",
        "Client wake up cancelled.",
      ),

    remote: (client) =>
      handleAction(
        client,
        "remote",
        "Remote Client",
        `Are you sure you want to remote client "${client.name}" ? `,
        "Remote Client",
        "remote_client",
        { clientId: client.id },
        "Client Remotely Connected",
        "Client remote connection cancelled.",
      ),

    reset: (client) =>
      handleAction(
        client,
        "reset",
        "Reset client writeback",
        `Are you sure you want to reset client "${client.name}" ? This will destroy their ZFS clone and remove configurations.`,
        "Reset Client",
        "reset_client",
        { clientId: client.id },
        "Client Reset successfully",
        "Client reset cancelled.",
      ),

    resetToClean: async (client) => {
      if (!client) return;

      // Check if client is in non-persistent mode
      if (client.keep_writeback !== false) {
        showNotification(
          "Client is in persistent mode. Only non-persistent clients can be reset to clean state.",
          "error",
        );
        return;
      }

      handleAction(
        client,
        "resetToClean",
        "Reset to Clean State",
        `This will delete the writeback for "${client.name}" and recreate it from the snapshot.All changes will be lost.Continue ? `,
        "Reset to Clean",
        "reset_client_to_clean",
        { clientId: client.id },
        "Client Reset to Clean successfully",
        "Reset to clean cancelled.",
      );
    },

    delete: (client) =>
      handleAction(
        client,
        "delete",
        "Delete Client",
        `Are you sure you want to delete client "${client.name}" ? This will destroy their ZFS clone and remove configurations.`,
        "Delete Client",
        "delete_client",
        { clientId: client.id },
        "Client Deleted successfully",
        "Client deletion cancelled.",
      ),

    enableSuper: (client) =>
      handleAction(
        client,
        "enableSuper",
        "Enable Super Client",
        `Client "${client.name}" will boot directly from master image.This skips clone / writeback.Continue ? `,
        "Enable Super",
        "control_client",
        { clientId: client.id, req: { action: "super", make_super: true } },
        "Client Enabled Super successfully",
        "Enable Super cancelled.",
      ),

    disableSuper: (client) => {
      if (client.mode !== "super") {
        showNotification("Client is not in Super mode.", "error");
        return;
      }
      handleAction(
        client,
        "disableSuper",
        "Disable Super Client",
        `This will point ${client.name} back to its writeback clone.Continue ? `,
        "Disable Super",
        "control_client",
        { clientId: client.id, req: { action: "super", make_super: false } },
        "Client Disabled Super successfully",
        "Disable Super cancelled.",
      );
    },

    saveSuper: async (client) => {
      if (!client) return;
      if (client.mode !== "super") {
        showNotification("Client is not in Super mode.", "error");
        return;
      }
      if (client.status !== "Offline") {
        showNotification("Client must be offline to save Super.", "error");
        return;
      }

      const ok = await confirm({
        title: "Save Super Client",
        description: `This will save the current state of ${client.name} to a snapshot.Continue ? `,
        confirmText: "Save Super",
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "2xl",
      });

      if (!ok) {
        showNotification("Save Super cancelled.", "info");
        return;
      }

      const suffix = window.prompt(
        "Enter snapshot name (alphanumeric, _ or -):",
        `${client.name} -super- ${Date.now()} `,
      );
      if (!suffix) {
        showNotification("Save Super cancelled.", "info");
        return;
      }
      if (!/^[-\w]+$/.test(suffix)) {
        showNotification("Invalid snapshot name.", "error");
        return;
      }

      const snapshotName = `${client.master} @${suffix} `;
      const token = localStorage.getItem("authToken") || "";

      try {
        const response = await invoke("create_snapshot", {
          token,
          snapshotName,
        });
        if (response.message) showNotification(response.message, "success");
        fetchData();
        closeContextMenu();
      } catch (error) {
        showNotification(
          "error",
          "Failed to save super",
          error.message || String(error),
        );
      }
    },
  };
};
