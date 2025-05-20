import React, { useState } from 'react';
import { Button } from '../Button';
import { Modal } from '../Modal';
import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '../../../contexts/NotificationContext';

const ClientFormModal = ({client, setClient, masters, isOpen, setIsOpen, refresh}) => {
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
          await invoke('add_client', {req: client}).then((response) => {
            if (response.message) showNotification(response.message, 'success');
            }).catch((error) => {
              showNotification(error, 'error',)
            }).finally(() => {
            refresh();
            });
        } else {
          showNotification(`Editing client ${client.name}`, 'info');
          await invoke('edit_client', {clientId: client.id, data:{
              name: client.name,
              mac: client.mac,
              ip: client.ip,
              master: client.master,
              snapshot: client.snapshot ? `${client.snapshot}` : null
            }}).then((response) => {
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
          <Button type="button" onClick={(event) => handleSubmit(event)} className="bg-blue-600 hover:bg-blue-700 text-white">{client.id ? 'Edit Client' : 'Add Client'}</Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;