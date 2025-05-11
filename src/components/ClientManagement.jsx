import {
  PlusCircle,
  Power, PowerOff,
  Users,
  Zap,
  Monitor
} from 'lucide-react';
import { useCallback, useState } from 'react';
import { Button, Card, ContextMenu, Modal } from '../components/ui';
import { useNotification } from '../contexts/NotificationContext';
import { apiRequest, handleApiAction } from '../utils/apiRequest';

export const ClientManagement = ({ clients, masters, fetchData }) => {
  const {showNotification} = useNotification();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [client, setClient] = useState({
    name: '',
    mac: '',
    ip: '',
    master: '',
    snapshot: '',
    clone: ''
  });  

  const handleClientFormSubmit = async (event) => {
    event.preventDefault();
    // Input validation
    if (!client.name.trim()) {
      showNotification('Client name is required.', 'error');
      return;
    }
    
    if (!client.mac.trim()) {
      showNotification('MAC address is required.', 'error');
      return;
    }
    
    if (!client.ip.trim()) {
      showNotification('IP address is required.', 'error');
      return;
    }
    
    if (!client.master) {
      showNotification('Please select a master image.', 'error');
      return;
    }
    
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
    console.log(client)

    setIsModalOpen(false);

    if(!client.id){
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
    }else{
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
    fetchData();
  };


  const handleClientFormOpen = () => {        
    setClient({
      name: 'pc002',
      mac: 'd8:43:ae:a7:8e:a8',
      ip: '192.168.1.101',
    });  
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
        setClient({
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
        fetchData();
      }
    },
  };


  return (
    <div className="mb-2 md:mb-4">
      <Card title="Client Management" icon={Users} actions={ // Add button to card actions
        <Button onClick={handleClientFormOpen} icon={PlusCircle} disabled={masters.length === 0}>
            Add Client {masters.length === 0 && <span className="text-xs text-red-500 ml-2 self-center">(Requires Master Image)</span>}
        </Button>
      }>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead className="hidden md:table-cell">MAC Address</TableHead>
              <TableHead>IP Address</TableHead>
              <TableHead className="hidden md:table-cell">Master</TableHead>
              <TableHead className="hidden xl:table-cell">Snapshot</TableHead>

              <TableHead className="hidden xl:table-cell">Writeback</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Mode</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {clients.map((client) => (
              <TableRow key={client.id} onContextMenu={(e) => handleClientContextMenu(e, client)} className="cursor-context-menu">
                <TableCell className="font-medium">
                  <Monitor className="inline mr-2 h-4 w-4" />
                  {client.name}</TableCell>
                <TableCell className="hidden md:table-cell text-xs font-mono">{client.mac}</TableCell>
                <TableCell>{client.ip}</TableCell>
                <TableCell className="hidden md:table-cell text-xs font-mono break-all">{client.master}</TableCell>
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
      <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} title={client.id ? 'Edit Client' : 'Add Client'}>
        <form onSubmit={handleClientFormSubmit} className="space-y-4">
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
              onChange={(e) => {
                // Update both the client state and selected master
                setClient({ 
                  ...client, 
                  master: e.target.value,
                  snapshot: '' // Reset snapshot when master changes
                });
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
              value={client.snapshot}
              onChange={(e) => {
                setClient({ ...client, snapshot: e.target.value });
              }}
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
            <Button type="button" onClick={() => setIsModalOpen(false)} variant="outline">Cancel</Button>
            <Button type="submit" className="bg-blue-600 hover:bg-blue-700 text-white">{client.id ? 'Edit Client' : 'Add Client'}</Button>
          </div>
        </form>
      </Modal>
    </div>
  );
};

export default ClientManagement;
