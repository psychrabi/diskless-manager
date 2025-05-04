import React, { useState } from 'react';
import {
  PlusCircle, Users, Edit, Trash2, Power, PowerOff, Zap, PowerSquare, Save
} from 'lucide-react';
import { useClientManager } from '../hooks/useClientManager';
import { Card, Button, Modal, Input, Select, Table } from '../components/ui';

export const ClientManagement = ({ clients, masters, fetchData }) => {
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
  } = useClientManager(clients, masters, fetchData);

  const handleOpenAddClientModal = () => {
    setNewClient({ name: '', mac: '', ip: '', snapshot: '' });
    setIsModalOpen(true);
  };

    // Modal States
    
  const [newClientName, setNewClientName] = useState('');
  const [newClientMac, setNewClientMac] = useState('');
  const [newClientIp, setNewClientIp] = useState('');
  const [selectedMaster, setSelectedMaster] = useState('');
  const [selectedSnapshot, setSelectedSnapshot] = useState('');

  const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
  const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-gray-200 dark:border-gray-700 ${className}`}>{children}</thead>;
  const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
  const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-gray-200 dark:border-gray-700 transition-colors hover:bg-gray-100/50 dark:hover:bg-gray-800/50 ${className}`}>{children}</tr>;
  const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-gray-500 dark:text-gray-400 ${className}`}>{children}</th>;
  const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;



  return (
    <Card title="Client Management" icon={Users} actions={ // Add button to card actions
      <Button onClick={handleOpenAddClientModal} icon={PlusCircle} disabled={masters.length === 0}>
          Add Client
          Image {masters.length === 0 &&
              <span className="text-xs text-red-500 ml-2 self-center">(Requires Master Image)</span>}
      </Button>
    }>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead className="hidden md:table-cell">MAC Address</TableHead>
            <TableHead>IP Address</TableHead>
            <TableHead className="hidden xl:table-cell">ZFS Clone</TableHead>
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
      {clients.length === 0 && !loading && <p className="text-center py-4 text-gray-500">No clients configured.</p>}
      {/* Add/Edit Client Modal */}
      <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={newClient.id ? 'Edit Client' : 'Add Client'}>
        <form onSubmit={handleAddNewClientSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Client Name</label>
            <input
              type="text"
              value={newClientName}
              onChange={(e) => setNewClientName(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="Enter client name"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">MAC Address</label>
            <input
              type="text"
              value={newClientMac}
              onChange={(e) => setNewClientMac(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="XX:XX:XX:XX:XX:XX"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">IP Address</label>
            <input
              type="text"
              value={newClientIp}
              onChange={(e) => setNewClientIp(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              placeholder="X.X.X.X"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-1">Master Image</label>
            <select
              value={selectedMaster}
              onChange={(e) => {
                setSelectedMaster(e.target.value);
                // Clear snapshot selection when master changes
                setSelectedSnapshot('');
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
              value={selectedSnapshot}
              onChange={(e) => setSelectedSnapshot(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            >
              <option value="">Use master directly</option>
              {masters.find(m => m.name === selectedMaster)?.snapshots?.map((snap) => (
                <option key={snap.name} value={snap.name}>
                  {snap.name} ({snap.created}, {snap.size})
                </option>
              ))}
            </select>
          </div>

          <div className="flex justify-end space-x-3">
            <Button type="button" onClick={() => setIsAddClientModalOpen(false)} variant="outline">Cancel</Button>
            <Button type="submit" className="bg-blue-600 hover:bg-blue-700 text-white">Add Client</Button>
          </div>
        </form>
      </Modal>
    </Card>
  );
};

export default ClientManagement;
