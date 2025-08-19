import { useConfirm } from "@/contexts/ConfirmDialogContext";
import { invoke } from "@tauri-apps/api/core";
import { useNotification } from "../contexts/NotificationContext";

export const clientContextMenuActions = (fetchData, closeContextMenu, setClient, setIsModalOpen) => {
  const { showNotification } = useNotification();
  const confirm = useConfirm()

  return {
    edit: (client) => {
      if (client.status === 'Online') { showNotification('Client must be offine to make changes.', 'error'); return; }
      setClient(client);
      setIsModalOpen(true);
      closeContextMenu();
    },
    reboot: async (client) => {
      if (client.status !== 'Online') { showNotification('Client must be online to reboot.', 'error'); return; }
      // Get token from localStorage
      const ok = await confirm({
        title: 'Reboot Client',
        description: `Are you sure you want to reboot client "${client.name}"?`,
        confirmText: 'Reboot Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        const token = localStorage.getItem('authToken') || '';
        await invoke('control_client', {
          token,
          clientId: client.id,
          req: { action: 'reboot' }
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
      }else {
        showNotification('Client reboot cancelled.', 'info');
        closeContextMenu();
      }
    },
    shutdown: async (client) => {
      if (client.status !== 'Online') { showNotification('Client must be online to shutdown.', 'error'); return; }
      // Get token from localStorage
      const ok = await confirm({
        title: 'Shutdown Client',
        description: `Are you sure you want to shutdown client "${client.name}"?`,
        confirmText: 'Shutdown Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        const token = localStorage.getItem('authToken') || '';
        await invoke('control_client',
          {
            token,
            clientId: client.id, req: { action: 'shutdown' }
          }).then((response) => {
            if (response.message) showNotification(response.message, 'success');
          }).catch((error) => showNotification(error, 'error'));
      }else {
        showNotification('Client shutdown cancelled.', 'info');
        closeContextMenu();
      }
    },
    wake: async (client) => {
      if (client.status === 'Online') { showNotification('Client must be offline to wake', 'error'); return; }
      // Get token from localStorage
      const ok = await confirm({
        title: 'Wake Client',
        description: `Are you sure you want to wake client "${client.name}"?`,
        confirmText: 'Wake Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        const token = localStorage.getItem('authToken') || '';
        await invoke('control_client', {
          token,
          clientId: client.id,
          req: { action: 'wake' }
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
      }else {
        showNotification('Client wake up cancelled.', 'info');
        closeContextMenu();
      }
    },
    remote: async (client) => {
      if (client.status !== 'Online') { showNotification('Client must be online to connect remotely', 'error'); return; }
      // Get token from localStorage
      const ok = await confirm({
        title: 'Remote Client',
        description: `Are you sure you want to remote client "${client.name}"?`,
        confirmText: 'Remote Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        const token = localStorage.getItem('authToken') || '';
        await invoke('remote_client', {
          token,
          clientId: client.id,
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
      }else {
        showNotification('Client remote connection cancelled.', 'info');
        closeContextMenu();
      }
    },
    reset: async (client) => {
      if (!client) return;
      if (client.status !== 'Offline') { showNotification('Client must be offline before you can reset', 'error'); return; }
      const ok = await confirm({
        title: 'Reset client writeback',
        description: `Are you sure you want to reset client "${client.name}"? This will destroy their ZFS clone and remove configurations.`,
        confirmText: 'Reset Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        // Get token from localStorage
        const token = localStorage.getItem('authToken') || '';
        await invoke('reset_client', {
          token,
          clientId: client.id,
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
      } else {
        showNotification('Client reset cancelled.', 'info');
        closeContextMenu();
      }      
    },
    delete: async (client) => {
      if (!client) return;
      if (client.status !== 'Offline') { showNotification('Client must be offline to delete.', 'error'); return; }
      const ok = await confirm({
        title: 'Delete Client',
        description: `Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`,
        confirmText: 'Delete Client',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (ok) {
        // Get token from localStorage
        const token = localStorage.getItem('authToken') || '';
        invoke('delete_client', { token, clientId: client.id })
          .then((response) => {
            if (response.message) showNotification(response.message, 'success');
          }).catch((error) => showNotification(error, 'error'))
          .finally(() => {
            closeContextMenu();
            // Refresh the data after deletion
            fetchData();
          });
      } else {
        showNotification('Client deletion cancelled.', 'info');
        closeContextMenu();
      }
    },
    enableSuper: async (client) => {
      if (!client) return;
      if (client.status !== 'Offline') { showNotification('Client must be offline to enable Super mode.', 'error'); return; }
      const ok = await confirm({
        title: 'Enable Super Client',
        description: `Client "${client.name}" will boot directly from master image. This skips clone/writeback. Continue?`,
        confirmText: 'Enable Super',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (!ok) { showNotification('Enable Super cancelled.', 'info'); return; }
      const token = localStorage.getItem('authToken') || '';
      await invoke('control_client', {
        token,
        clientId: client.id,
        req: { action: 'super', make_super: true }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
        fetchData();
      }).catch((error) => showNotification(error, 'error'));
    },
    disableSuper: async (client) => {
      if (!client) return;
      if (client.mode !== 'super') { showNotification('Client is not in Super mode.', 'error'); return; }
      if (client.status !== 'Offline') { showNotification('Client must be offline to disable Super mode.', 'error'); return; }
      const ok = await confirm({
        title: 'Disable Super Client',
        description: `This will point ${client.name} back to its writeback clone. Continue?`,
        confirmText: 'Disable Super',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (!ok) { showNotification('Disable Super cancelled.', 'info'); return; }
      const token = localStorage.getItem('authToken') || '';
      await invoke('control_client', {
        token,
        clientId: client.id,
        req: { action: 'super', make_super: false }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
        fetchData();
        closeContextMenu();
      }).catch((error) => showNotification(error, 'error'));
    },
    saveSuper: async (client) => {
      if (!client) return;
      if (client.mode !== 'super') { showNotification('Client is not in Super mode.', 'error'); return; }
      if (client.status !== 'Offline') { showNotification('Client must be offline to save Super.', 'error'); return; }
      const ok = await confirm({
        title: 'Save Super Client',
        description: `This will save the current state of ${client.name} to a snapshot. Continue?`,
        confirmText: 'Save Super',
        cancelText: 'Cancel',
        confirmVariant: 'primary',
        size: '2xl'
      });
      if (!ok) { showNotification('Save Super cancelled.', 'info'); return; }

      const suffix = window.prompt('Enter snapshot name (alphanumeric, _ or -):', `${client.name}-super-${Date.now()}`);
      if (!suffix) { showNotification('Save Super cancelled.', 'info'); return; }
      if (!/^[-\w]+$/.test(suffix)) { showNotification('Invalid snapshot name.', 'error'); return; }
      const snapshotName = `${client.master}@${suffix}`;
      const token = localStorage.getItem('authToken') || '';
      await invoke('create_snapshot', { token, snapshotName })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
          // refresh list so UI reflects any snapshot changes
          fetchData();
          closeContextMenu();
        })
        .catch((error) => showNotification(error, 'error'));
    },
    // deprovision: (client) => {
    //   // Open deprovision modal with client data
    //   setDeprovisionModal({ isOpen: true, client: client });
    //   closeContextMenu();
    // }
  }
};
