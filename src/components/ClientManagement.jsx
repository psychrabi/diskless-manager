import React, { useState, useCallback } from 'react';
import {
  PlusCircle, Users, Edit, Trash2, Power, PowerOff, Zap, PowerSquare, Save
} from 'lucide-react';
import { useClientManager } from '../hooks/useClientManager';
import { Card, Button, Modal, Input, Select, Table, ContextMenu } from '../components/ui';
import { apiRequest, handleApiAction } from '../utils/apiRequest';
import { useNotification } from '../contexts/NotificationContext';

export const ClientManagement = ({ clients, masters, fetchData }) => {
  const {showNotification} = useNotification();

  const {
    isModalOpen,
    newClient,
    setNewClient,
    setIsModalOpen,
    validateClient,
    handleAddNewClientSubmit,
    handleEditClient,
    handleDeleteClient,
    handleToggleSuperClient
  } = useClientManager(clients, masters, fetchData, showNotification);
      
  const [selectedMaster, setSelectedMaster] = useState('');
  const [selectedSnapshot, setSelectedSnapshot] = useState('');

  const handleOpenAddClientModal = () => {        
    setNewClient({
      name: 'pc002',
      mac: 'd8:43:ae:a7:8e:a8',
      ip: '192.168.1.101',
      master: '',
      snapshot: ''
    });
    // Set default master selection if available
    if (masters.length > 0) {
        setSelectedMaster(masters[0].name);
        // Set default snapshot if available
        if (masters[0].snapshots?.length > 0) {
            setSelectedSnapshot(masters[0].snapshots[masters[0].snapshots.length - 1].name);
        }
    }
    setIsModalOpen(true);
  };

  const [contextMenu, setContextMenu] = useState({ isOpen: false, x: 0, y: 0, client: null });

  const handleClientContextMenu = (event, client) => {
    event.preventDefault();
    setContextMenu({ isOpen: true, x: event.clientX, y: event.clientY, client: client });
  };

  const closeContextMenu = useCallback(() => {
      setContextMenu(prev => ({ ...prev, isOpen: false }));
  }, []);
    


  const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
  const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-gray-200 dark:border-gray-700 ${className}`}>{children}</thead>;
  const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
  const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-gray-200 dark:border-gray-700 transition-colors hover:bg-gray-100/50 dark:hover:bg-gray-800/50 ${className}`}>{children}</tr>;
  const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-gray-500 dark:text-gray-400 ${className}`}>{children}</th>;
  const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;

  const clientContextMenuActions = {
    edit: (client) => {
        // Open the add client modal with current client data
        console.log('Client object:', client);
        
        // Set the client data with current master and snapshot
        setNewClient({
            ...client,
            master: client.master || '',
            snapshot: client.snapshot || ''
        });
        
        // Extract master and snapshot information from paths
        const masterPath = client.master || '';
        const snapshotPath = client.snapshot || '';
        
        // Extract master name from path (e.g., tank/win10-master -> win10-master)
        const masterName = masterPath.split('/').pop() || '';
        
        // Extract snapshot name from path (e.g., tank/win10-master@2025-05-01 -> 2025-05-01)
        const snapshotName = snapshotPath.includes('@') 
          ? snapshotPath.split('@')[1] 
          : '';
        
        setSelectedMaster(masterName);
        setSelectedSnapshot(snapshotName);
        setIsModalOpen(true);
        
        // Close the context menu
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
    reboot: (client) => {
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'reboot' }),
            `Reboot command sent to ${client.name}.`,
            `Failed to send reboot command to ${client.name}`,
            showNotification
        );
    },
    shutdown: (client) => {
         handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'shutdown' }),
            `Shutdown command sent to ${client.name}.`,
            `Failed to send shutdown command to ${client.name}`,
            showNotification
        );
    },
    wake: (client) => {
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'wake' }),
            `Wake-on-LAN command sent for ${client.name}.`,
            `Failed to send Wake-on-LAN for ${client.name}`,
            showNotification
        );
    },
    delete: (client) => {
      if (confirm(`Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`)) {
         handleApiAction(
            () => apiRequest(`/clients/${client.id}`, 'DELETE'),
            `Client ${client.name} deleted successfully.`,
            `Failed to delete client ${client.name}`,
            showNotification
        );
      }
    },
  };


  return (
    <div className="mb-2 md:mb-4">
      <Card title="Client Management" icon={Users} actions={ // Add button to card actions
        <Button onClick={handleOpenAddClientModal} icon={PlusCircle} disabled={masters.length === 0}>
            Add Client {masters.length === 0 && <span className="text-xs text-red-500 ml-2 self-center">(Requires Master Image)</span>}
        </Button>
      }>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead className="hidden md:table-cell">MAC Address</TableHead>
              <TableHead>IP Address</TableHead>
              <TableHead className="hidden xl:table-cell">Master</TableHead>
              <TableHead className="hidden xl:table-cell">Snapshot</TableHead>

              <TableHead className="hidden xl:table-cell">Writeback</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Mode</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {clients.map((client) => (
              <TableRow key={client.id} onContextMenu={(e) => handleClientContextMenu(e, client)} className="cursor-context-menu">
                <TableCell className="font-medium">{client.name}</TableCell>
                <TableCell className="hidden md:table-cell text-xs font-mono">{client.mac}</TableCell>
                <TableCell>{client.ip}</TableCell>
                <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.master}</TableCell>
                <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.snapshot}</TableCell>
                <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.clone}</TableCell>
                <TableCell>
                  <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${client.status === 'Online' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'}`}>
                    {client.status === 'Online' ? <Power className="h-3 w-3 mr-1 text-green-500"/> : <PowerOff className="h-3 w-3 mr-1 text-gray-500"/>}
                    {client.status}
                  </span>
                </TableCell>
                <TableCell>
                  {client.isSuperClient && (
                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200" title="Changes persist directly to the clone">
                      <Zap className="h-3 w-3 mr-1 text-yellow-500"/> Super
                    </span>
                  )}
                  {!client.isSuperClient && (
                    <span className="text-xs text-gray-500 dark:text-gray-400">Normal</span>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {clients.length === 0 && <p className="text-center py-4 text-gray-500">No clients configured.</p>}
        
        {/* Client Context Menu */}
        <ContextMenu
          isOpen={contextMenu.isOpen}
          xPos={contextMenu.x}
          yPos={contextMenu.y}
          targetClient={contextMenu.client}
          onClose={closeContextMenu}
          actions={clientContextMenuActions}
        />
      </Card>
      {/* Add/Edit Client Modal */}
      <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={newClient.id ? 'Edit Client' : 'Add Client'}>
        <form onSubmit={handleAddNewClientSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Client Name</label>
            <input
              type="text"
              value={newClient.name}
              onChange={(e) => setNewClient({ ...newClient, name: e.target.value })}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="Enter client name"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">MAC Address</label>
            <input
              type="text"
              value={newClient.mac}
              onChange={(e) => setNewClient({ ...newClient, mac: e.target.value })}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="XX:XX:XX:XX:XX:XX"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">IP Address</label>
            <input
              type="text"
              value={newClient.ip}
              onChange={(e) => setNewClient({ ...newClient, ip: e.target.value })}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="X.X.X.X"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">Master Image</label>
            <select
              value={newClient.master}
              onChange={(e) => {
                const selectedMaster = e.target.value;
                // Update both the client state and selected master
                setNewClient({ 
                  ...newClient, 
                  master: selectedMaster,
                  snapshot: '' // Reset snapshot when master changes
                });
                setSelectedMaster(selectedMaster);
                setSelectedSnapshot(''); // Reset selected snapshot
              }}
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
              value={newClient.snapshot}
              onChange={(e) => {
                setNewClient({ ...newClient, snapshot: e.target.value });
                setSelectedSnapshot(e.target.value);
              }}
              disabled={!newClient.master}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            >
              <option value="">Use master directly</option>
              {masters.find(m => m.name === newClient.master)?.snapshots?.map((snap) => (
                <option key={snap.name} value={snap.name}>
                  {snap.name} ({snap.created}, {snap.size})
                </option>
              ))}
            </select>
          </div>

          <div className="flex justify-end space-x-3">
            <Button type="button" onClick={() => setIsModalOpen(false)} variant="outline">Cancel</Button>
            <Button type="submit" className="bg-blue-600 hover:bg-blue-700 text-white">{newClient.id ? 'Edit Client' : 'Add Client'}</Button>
          </div>
        </form>
      </Modal>
    </div>
  );
};

export default ClientManagement;
