import { useNotification } from "../contexts/NotificationContext";
import { invoke } from "@tauri-apps/api/core";
import { apiRequest, handleApiAction } from '../utils/apiRequest';

export const clientContextMenuActions = (fetchData, closeContextMenu, setClient, setIsModalOpen) => {
  const { showNotification } = useNotification();

  return {
    edit: (client) => {
      // Open the add client modal with current client data
      console.log('Client object:', client);
      setClient(client);
      setIsModalOpen(true);
      closeContextMenu();
    },
    toggleSuper: (client) => {
      const makeSuper = !client.isSuperClient;
      handleApiAction(
        () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'toggleSuper', makeSuper: makeSuper }),
        `Super Client mode ${makeSuper ? 'enabled' : 'disabled'} for ${client.name}.`,
        `Failed to toggle Super Client mode for ${client.name}`,
        showNotification
      );
    },
    reboot: async (client) => {
      await invoke('control_client', {
        clientId: client.id,
        req: { action: 'reboot' }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));

    },
    shutdown: async (client) => {
      await invoke('control_client',
        {
          clientId: client.id, req: { action: 'shutdown' }
        }).then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => showNotification(error, 'error'));
    },
    wake: async (client) => {
      if (client.status !== 'Offline') { showNotification('Client must be offline to wake', 'error'); return; }
      await invoke('control_client', {
        clientId: client.id,
        req: { action: 'wake' }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    remote: async (client) => {
      if (client.status !== 'Online') {
        showNotification('Client must be online to connect remotely', 'error');
        return;
      }
      await invoke('remote_client', {
        clientId: client.id,
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => showNotification(error, 'error'));
    },
    reset: (client) => {
      if (client.status !== 'Offline') {
        showNotification('Client must be offline before you can reset', 'error');
        return;
      }

      handleApiAction(
        () => apiRequest(`/clients/reset/${client.id}`, 'POST'),
        'Resetting the client...',
        'Failed to reset the client',
        showNotification
      )
    },
    delete: (client) => {
      console.log(client)
      if (confirm(`Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`)) {
        handleApiAction(
          () => apiRequest(`/clients/${client.name}`, 'DELETE'),
          `Client ${client.name} deleted successfully.`,
          `Failed to delete client ${client.name}`,
          showNotification
        );
        fetchData();
      }
    }
  }
};
