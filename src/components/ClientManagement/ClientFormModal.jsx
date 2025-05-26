import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '../../contexts/NotificationContext';
import { Button, Input, Modal, Select } from '../ui';

const ClientFormModal = ({ client, setClient, masters, isOpen, setIsOpen, refresh }) => {
  const { showNotification } = useNotification();

  const handleSubmit = async (event) => {
    event.preventDefault();

    // Validate MAC address format
    const macRegex = /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/;
    if (!macRegex.test(client.mac)) {
      showNotification('Invalid MAC address format. Use XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX', 'error');
      return;
    }

    // Validate IP address format
    const ipRegex = /^([\d]{1,3}\.){3}\d{1,3}$/;
    if (!ipRegex.test(client.ip)) {
      showNotification('Invalid IP address format. Use X.X.X.X', 'error');
      return;
    }

    setIsOpen(false);

    if (!client.id) {
      showNotification(`Adding new client ${client.name}`, 'info');
      await invoke('add_client', { req: client }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        refresh();
      });
    } else {
      showNotification(`Editing client ${client.name}`, 'info');
      await invoke('edit_client', {
        clientId: client.id, data: {
          name: client.name,
          mac: client.mac,
          ip: client.ip,
          master: client.master,
          snapshot: client.snapshot ? `${client.snapshot}` : null
        }
      }).then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        refresh();
      });

    }

  };

  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title={client.id ? 'Edit Client' : 'Add Client'}>
      <form onSubmit={() => handleSubmit} className="space-y-2">
        <Input
          label="Client name"
          id="snapshotName"
          value={client.name}
          onChange={(e) => setClient({ ...client, name: e.target.value })}
          placeholder="Enter client name"
          required
        />

        <Input
          label="MAC Address"
          value={client.mac}
          onChange={(e) => setClient({ ...client, mac: e.target.value })}
          placeholder="XX:XX:XX:XX:XX:XX"
        />
        <Input
          label="IP Address"
          value={client.ip}
          onChange={(e) => setClient({ ...client, ip: e.target.value })}
          placeholder="X.X.X.X"
        />

        <Select
          value={client.master}
          onChange={(e) => setClient({ ...client, master: e.target.value, snapshot: '' })}
          label="Select master"
        >
          <option value="">Select a master image...</option>
          {masters.map((master) => (
            <option key={master.name} value={master.name}>
              {master.name}
            </option>
          ))}
        </Select>
        <Select
          value={client.snapshot}
          onChange={(e) => setClient({ ...client, snapshot: e.target.value })}
          disabled={!client.master}
          label="Select Snapshot"
        >
          <option value="">Use master directly</option>
          {masters.find(m => m.name === client.master)?.snapshots?.map((snap) => (
            <option key={snap.name} value={snap.name}>
              {snap.name} ({snap.created}, {snap.size})
            </option>
          ))}
        </Select>
        <div className="flex justify-end space-x-3">
          <Button type="button" onClick={(event) => handleSubmit(event)} variant="primary">{client.id ? 'Edit Client' : 'Add Client'}</Button>
          <Button type="button" onClick={() => setIsOpen(false)} variant="destructive">Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;