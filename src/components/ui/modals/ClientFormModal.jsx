import React, { useState } from 'react';
import { Button } from '../Button';
import { Modal } from '../Modal';

const ClientFormModal = ({client, setClient, masters, isOpen, setIsOpen, refresh}) => {
  

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
          console.log("Adding new client")
          await handleApiAction(
            () => apiRequest('/clients', 'POST', {
              name: client.name,
              mac: client.mac,
              ip: client.ip,
              master: client.master,
              snapshot: client.snapshot ? `${client.snapshot}` : null
            }),
            `Client ${client.name} added successfully.`,
            `Failed to add client ${client.name}`,
            showNotification
          );
        } else {
          console.log("Editing client")
          await handleApiAction(
            () => apiRequest(`/clients/edit/${client.id}`, 'POST', {
              name: client.name,
              mac: client.mac,
              ip: client.ip,
              master: client.master,
              snapshot: client.snapshot ? `${client.snapshot}` : null
            }),
            `Client ${client.name} updated successfully.`,
            `Failed to update client ${client.name}`,
            showNotification
          );
        }
        refresh();
      };

  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title={client.id ? 'Edit Client' : 'Add Client'}>
      <form onSubmit={() => handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1">Client Name</label>
          <input
            type="text"
            value={client.name}
            onChange={(e) => setClient({ ...client, name: e.target.value })}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            placeholder="Enter client name"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">MAC Address</label>
          <input
            type="text"
            value={client.mac}
            onChange={(e) => setClient({ ...client, mac: e.target.value })}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            placeholder="XX:XX:XX:XX:XX:XX"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">IP Address</label>
          <input
            type="text"
            value={client.ip}
            onChange={(e) => setClient({ ...client, ip: e.target.value })}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            placeholder="X.X.X.X"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">Master Image</label>
          <select
            value={client.master}
            onChange={(e) => setClient({ ...client, master: e.target.value, snapshot: '' })}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value="">Select a master image...</option>
            {masters.map((master) => (
              <option key={master.name} value={master.name}>
                {master.name}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">Snapshot (Optional)</label>
          <select
            value={client.snapshot}
            onChange={(e) => setClient({ ...client, snapshot: e.target.value })}
            disabled={!client.master}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value="">Use master directly</option>
            {masters.find(m => m.name === client.master)?.snapshots?.map((snap) => (
              <option key={snap.name} value={snap.name}>
                {snap.name} ({snap.created}, {snap.size})
              </option>
            ))}
          </select>
        </div>
        <div className="flex justify-end space-x-3">
          <Button type="button" onClick={() => setIsOpen(false)} variant="outline">Cancel</Button>
          <Button type="submit" className="bg-blue-600 hover:bg-blue-700 text-white">{client.id ? 'Edit Client' : 'Add Client'}</Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;