import React from 'react';
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
    handleAddClient,
    handleEditClient,
    handleDeleteClient,
    handleToggleSuperClient
  } = useClientManager(clients, masters, fetchData);

  const handleOpenAddClientModal = () => {
    setNewClient({ name: '', mac: '', ip: '', snapshot: '' });
    setIsModalOpen(true);
  };

  return (
    <Card title="Client Management" icon={Users}>
      <div className="mb-4 flex justify-end">
        <Button onClick={handleOpenAddClientModal} icon={PlusCircle}>Add New Client</Button>
      </div>

      <Table
        data={clients}
        columns={[
          { key: 'name', label: 'Name', width: 'w-48' },
          { key: 'mac', label: 'MAC Address', width: 'w-48' },
          { key: 'ip', label: 'IP Address', width: 'w-48' },
          { key: 'snapshot', label: 'Snapshot', width: 'w-48' },
          { key: 'super', label: 'Super', width: 'w-24' },
          { key: 'actions', label: 'Actions', width: 'w-32' }
        ]}
        renderCell={(row, column) => {
          switch (column.key) {
            case 'super':
              return (
                <Button
                  onClick={() => handleToggleSuperClient(row.id)}
                  variant={row.super ? 'default' : 'outline'}
                  size="icon"
                  className="h-7 w-7"
                >
                  <PowerSquare className="h-4 w-4" />
                </Button>
              );
            case 'actions':
              return (
                <div className="flex space-x-1">
                  <Button
                    onClick={() => {
                      setNewClient(row);
                      setIsModalOpen(true);
                    }}
                    variant="outline"
                    size="icon"
                    className="h-7 w-7"
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
                  <Button
                    onClick={() => handleDeleteClient(row.id)}
                    variant="destructive"
                    size="icon"
                    className="h-7 w-7"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                  <Button
                    onClick={() => handleToggleSuperClient(row.id)}
                    variant="outline"
                    size="icon"
                    className="h-7 w-7"
                  >
                    <Power className="h-4 w-4" />
                  </Button>
                </div>
              );
            default:
              return row[column.key];
          }
        }}
      />

      {/* Add/Edit Client Modal */}
      <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={newClient.id ? 'Edit Client' : 'Add Client'}>
        <form onSubmit={(e) => {
          e.preventDefault();
          if (validateClient()) {
            if (newClient.id) {
              handleEditClient(newClient).catch((error) => {
                console.error('Failed to edit client:', error);
              });
            } else {
              handleAddClient(newClient).catch((error) => {
                console.error('Failed to add client:', error);
              });
            }
            setIsModalOpen(false);
          }
        }}>
          <Input
            label="Client Name:"
            id="name"
            value={newClient.name}
            onChange={(e) => setNewClient(prev => ({ ...prev, name: e.target.value }))}
            required
          />
          <Input
            label="MAC Address:"
            id="mac"
            value={newClient.mac}
            onChange={(e) => setNewClient(prev => ({ ...prev, mac: e.target.value }))}
            required
            pattern="^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$"
          />
          <Input
            label="IP Address:"
            id="ip"
            value={newClient.ip}
            onChange={(e) => setNewClient(prev => ({ ...prev, ip: e.target.value }))}
            required
            pattern="^((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$"
          />
          <Select
            label="Snapshot:"
            id="snapshot"
            value={newClient.snapshot}
            onChange={(e) => setNewClient(prev => ({ ...prev, snapshot: e.target.value }))}
            required
          >
            <option value="" disabled>-- Select Snapshot --</option>
            {masters.flatMap(master => master.snapshots).map(snap => (
              <option key={snap.id} value={snap.name}>{snap.name}</option>
            ))}
          </Select>
          <div className="mt-6 flex justify-end space-x-3">
            <Button type="button" variant="outline" onClick={() => setIsModalOpen(false)}>Cancel</Button>
            <Button type="submit" icon={Save}>{newClient.id ? 'Save Changes' : 'Add Client'}</Button>
          </div>
        </form>
      </Modal>
    </Card>
  );
};

export default ClientManagement;
