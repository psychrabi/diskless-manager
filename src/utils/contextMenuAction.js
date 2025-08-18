import { invoke } from "@tauri-apps/api/core";
import { useNotification } from "../contexts/NotificationContext";

export const clientContextMenuActions = (fetchData, closeContextMenu, setClient, setIsModalOpen, setDeprovisionModal, handleDeleteClient) => {
  const { showNotification } = useNotification();

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
      const token = localStorage.getItem('authToken') || '';
      await invoke('control_client', {
        token,
        clientId: client.id,
        req: { action: 'reboot' }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    shutdown: async (client) => {
      if (client.status !== 'Online') { showNotification('Client must be online to shutdown.', 'error'); return; }
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('control_client',
        {
          token,
          clientId: client.id, req: { action: 'shutdown' }
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
    },
    wake: async (client) => {
      if (client.status === 'Online') { showNotification('Client must be offline to wake', 'error'); return; }
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('control_client', {
        token,
        clientId: client.id,
        req: { action: 'wake' }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    remote: async (client) => {
      if (client.status !== 'Online') { showNotification('Client must be online to connect remotely', 'error'); return; }
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('remote_client', {
        token,
        clientId: client.id,
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    reset: async (client) => {
      if (client.status !== 'Offline') { showNotification('Client must be offline before you can reset', 'error'); return; }
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('reset_client', {
        token,
        clientId: client.id,
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    delete: (client) => {
      if (client.status !== 'Offline') { showNotification('Client must be offline to delete.', 'error'); return; }
      handleDeleteClient(client)
      // showNotification(`Deleting client... ${client.name}`, 'info');

      // if (confirm(`Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`)) {
      //   // Get token from localStorage
      //   const token = localStorage.getItem('authToken') || '';
      //   invoke('delete_client', { token, clientId: client.id })
      //     .then((response) => {
      //       if (response.message) showNotification(response.message, 'success');
      //     }).catch((error) => showNotification(error, 'error'))
      //     .finally(() => {
      //       closeContextMenu();
      //       // Refresh the data after deletion
      //       fetchData();
      //     });
      // } else {
      //   showNotification('Client deletion cancelled.', 'info');
      //   closeContextMenu();
      // }
    },
    deprovision: (client) => {
      // Open deprovision modal with client data
      setDeprovisionModal({ isOpen: true, client: client });
      closeContextMenu();
    }
  }
};
