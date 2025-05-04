import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  HardDrive, Server, Network, Users, Copy, Trash2, PlusCircle, RefreshCw, Power, PowerOff, Edit, Zap, PowerSquare, Sunrise, X, Save, Merge, GitBranchPlus, AlertCircle, FileText, Eye // Added icons
} from 'lucide-react';

// --- Configuration ---
const API_BASE_URL = 'http://192.168.1.206:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

// --- Helper Functions & Hooks ---


// --- UI Components (Keep existing components: Card, Button, Table, Modal, Input, Select, ContextMenu) ---
import { Card, Button, Modal, Input, ContextMenu } from './components/ui/index.js';
import ServiceManagement from './components/ServiceManagement.jsx';
import { apiRequest } from './utils/apiRequest.js';



const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-gray-200 dark:border-gray-700 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-gray-200 dark:border-gray-700 transition-colors hover:bg-gray-100/50 dark:hover:bg-gray-800/50 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-gray-500 dark:text-gray-400 ${className}`}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;


const Select = ({ label, id, value, onChange, children, required = false }) => (
     <div className="mb-4">
        <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{label}</label>
        <select
            id={id}
            name={id}
            value={value}
            onChange={onChange}
            required={required}
            className="block w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm focus:ring-blue-500 focus:border-blue-500 sm:text-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100"
        >
            {children}
        </select>
    </div>
);






// --- Main Application Component ---
function App() {
  const [clients, setClients] = useState([]);
  const [masters, setMasters] = useState([]);
  const [services, setServices] = useState({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [actionStatus, setActionStatus] = useState({ message: '', type: 'info' }); // For user feedback

  // Modal States
  const [isAddClientModalOpen, setIsAddClientModalOpen] = useState(false);
  const [newClientName, setNewClientName] = useState('');
  const [newClientMac, setNewClientMac] = useState('');
  const [newClientIp, setNewClientIp] = useState('');
  const [selectedMaster, setSelectedMaster] = useState('');
  const [selectedSnapshot, setSelectedSnapshot] = useState('');

  const [isCreateMasterModalOpen, setIsCreateMasterModalOpen] = useState(false); // New state for create master modal
  const [newMasterName, setNewMasterName] = useState('');
  const [newMasterSize, setNewMasterSize] = useState('50G'); // Default size



  // Context Menu State
  const [contextMenu, setContextMenu] = useState({ isOpen: false, x: 0, y: 0, client: null });

  // --- Data Fetching ---
  const fetchData = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    setError(null); // Clear previous errors
    setContextMenu({ isOpen: false, x: 0, y: 0, client: null });
    try {
      console.log("Fetching data...");
      const [clientsRes, mastersRes, servicesRes] = await Promise.all([
          apiRequest('/clients'),
          apiRequest('/masters'),
          apiRequest('/services')
      ]);

      console.log("Clients:", clientsRes);
      console.log("Masters:", mastersRes);
      console.log("Services:", servicesRes);

      setClients(clientsRes || []);
      setMasters(mastersRes || []);
      setServices(servicesRes || {});

      // Set default snapshot selection for Add Client modal only if not already set and snapshots exist
      if (!selectedSnapshot && mastersRes?.length > 0 && mastersRes[0].snapshots?.length > 0) {
         setSelectedSnapshot(mastersRes[0].snapshots[mastersRes[0].snapshots.length - 1].name);
      } else if (mastersRes?.flatMap(m => m.snapshots || []).length === 0) {
          setSelectedSnapshot(''); // Clear selection if no snapshots exist anywhere
      }
      console.log("Data fetched successfully.");
    } catch (err) {
      console.error("Failed to fetch data:", err);
      setError(`Failed to load data: ${err.message || 'Check backend connection.'}`);
    } finally {
      if (showLoading) setLoading(false);
    }
  }, [selectedSnapshot]); // Include selectedSnapshot dependency

  useEffect(() => {
    fetchData();
  }, [fetchData]); // Run once on mount

  // Clear action status message after a delay
  useEffect(() => {
      if (actionStatus.message) {
          const timer = setTimeout(() => {
              setActionStatus({ message: '', type: 'info' });
          }, 5000); // Clear after 5 seconds
          return () => clearTimeout(timer);
      }
  }, [actionStatus]);


  // --- Action Handlers ---
  const handleApiAction = async (actionFn, successMessage, errorMessagePrefix) => {
      setActionStatus({ message: 'Processing...', type: 'info' });
      try {
          const result = await actionFn();
          setActionStatus({ message: result?.message || successMessage, type: 'success' });
          fetchData(false); // Refresh data in the background after successful action
          return true; // Indicate success
      } catch (error) {
          setActionStatus({ message: `${errorMessagePrefix}: ${error.message || 'Unknown error'}`, type: 'error' });
          return false; // Indicate failure
      }
  };

  const handleRefresh = () => {
    console.log("Manual refresh triggered.");
    fetchData();
  };

  // --- Client Actions ---
  const handleOpenAddClientModal = () => {
    setNewClientName('pc001');
    setNewClientMac('d8:43:ae:a7:8e:a7');
    setNewClientIp('192.168.1.100');
    setSelectedMaster('');
    setSelectedSnapshot('');
    // Set default master selection if available
    if (masters.length > 0) {
        setSelectedMaster(masters[0].name);
        // Set default snapshot if available
        if (masters[0].snapshots?.length > 0) {
            setSelectedSnapshot(masters[0].snapshots[masters[0].snapshots.length - 1].name);
        }
    }
    setIsAddClientModalOpen(true);
  };

  const handleAddNewClientSubmit = async (event) => {
    event.preventDefault();
    
    // Input validation
    if (!newClientName.trim()) {
      setActionStatus({ message: 'Client name is required.', type: 'error' });
      return;
    }
    
    if (!newClientMac.trim()) {
      setActionStatus({ message: 'MAC address is required.', type: 'error' });
      return;
    }
    
    if (!newClientIp.trim()) {
      setActionStatus({ message: 'IP address is required.', type: 'error' });
      return;
    }
    
    if (!selectedMaster) {
      setActionStatus({ message: 'Please select a master image.', type: 'error' });
      return;
    }
    
    // Validate MAC address format
    const macRegex = /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/;
    if (!macRegex.test(newClientMac)) {
      setActionStatus({ message: 'Invalid MAC address format. Use XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX', type: 'error' });
      return;
    }
    
    // Validate IP address format
    const ipRegex = /^([\d]{1,3}\.){3}\d{1,3}$/;
    if (!ipRegex.test(newClientIp)) {
      setActionStatus({ message: 'Invalid IP address format. Use X.X.X.X', type: 'error' });
      return;
    }
    
    setIsAddClientModalOpen(false);
    await handleApiAction(
        () => apiRequest('/clients', 'POST', { 
            name: newClientName, 
            mac: newClientMac, 
            ip: newClientIp, 
            master: selectedMaster,
            snapshot: selectedSnapshot ? `${selectedMaster}@${selectedSnapshot}` : null
        }),
        `Client ${newClientName} added successfully.`,
        `Failed to add client ${newClientName}`
    );
  };

  const handleClientContextMenu = (event, client) => {
    event.preventDefault();
    setContextMenu({ isOpen: true, x: event.clientX, y: event.clientY, client: client });
  };

  const closeContextMenu = useCallback(() => {
      setContextMenu(prev => ({ ...prev, isOpen: false }));
  }, []);


  const clientContextMenuActions = {
    edit: (client) => {
        // Log the client object to debug its structure
        console.log('Client object:', client);
        
        // Open the add client modal with current client data
        setNewClientName(client.name);
        setNewClientMac(client.mac);
        setNewClientIp(client.ip);
        
        // Try to get master and snapshot information
        const masterInfo = client.master ? client.master.split('/') : [];
        const snapshotInfo = client.snapshot ? client.snapshot.split('@') : [];
        
        setSelectedMaster(masterInfo[1] || ''); // Extract master name from path if exists
        setSelectedSnapshot(snapshotInfo[1] || ''); // Extract snapshot name if exists
        setIsAddClientModalOpen(true);
        
        // Close the context menu
        closeContextMenu();
    },
    toggleSuper: (client) => {
        const makeSuper = !client.isSuperClient;
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'toggleSuper', makeSuper: makeSuper }),
            `Super Client mode ${makeSuper ? 'enabled' : 'disabled'} for ${client.name}.`,
            `Failed to toggle Super Client mode for ${client.name}`
        );
    },
    reboot: (client) => {
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'reboot' }),
            `Reboot command sent to ${client.name}.`,
            `Failed to send reboot command to ${client.name}`
        );
    },
    shutdown: (client) => {
         handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'shutdown' }),
            `Shutdown command sent to ${client.name}.`,
            `Failed to send shutdown command to ${client.name}`
        );
    },
    wake: (client) => {
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'wake' }),
            `Wake-on-LAN command sent for ${client.name}.`,
            `Failed to send Wake-on-LAN for ${client.name}`
        );
    },
    delete: (client) => {
      if (confirm(`Are you sure you want to delete client "${client.name}"? This will destroy their ZFS clone and remove configurations.`)) {
         handleApiAction(
            () => apiRequest(`/clients/${client.id}`, 'DELETE'),
            `Client ${client.name} deleted successfully.`,
            `Failed to delete client ${client.name}`
        );
      }
    },
  };

  // --- Master/Snapshot Actions ---
   const handleOpenCreateMasterModal = () => {
        setNewMasterName('');
        setNewMasterSize('50G'); // Reset to default
        setIsCreateMasterModalOpen(true);
    };

   const handleCreateMasterSubmit = async (event) => {
        event.preventDefault();
        setIsCreateMasterModalOpen(false); // Close modal
        await handleApiAction(
            () => apiRequest('/masters', 'POST', { name: newMasterName, size: newMasterSize }),
            `Master ZVOL ${newMasterName}-master created successfully.`,
            `Failed to create master ZVOL ${newMasterName}-master`
        );
    };

  const handleCreateSnapshot = (masterName) => {
      const snapSuffix = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-');
      const defaultSnapshotName = `${masterName}@auto-${snapSuffix}`;
      const snapshotName = prompt(`Enter name for new snapshot of ${masterName}:`, defaultSnapshotName);
      if (snapshotName) {
          handleApiAction(
              () => apiRequest('/snapshots', 'POST', { name: snapshotName }),
              `Snapshot ${snapshotName} created successfully.`,
              `Failed to create snapshot ${snapshotName}`
          );
      }
  };

  const handleDeleteSnapshot = (snapshotName) => {
       const encodedSnapshotName = encodeURIComponent(snapshotName);
       if (confirm(`Are you sure you want to delete snapshot "${snapshotName}"? This cannot be undone and might affect clones.`)) {
           handleApiAction(
              () => apiRequest(`/snapshots/${encodedSnapshotName}`, 'DELETE'),
              `Snapshot ${snapshotName} deleted successfully.`,
              `Failed to delete snapshot ${snapshotName}`
          );
      }
  };

   const handleCloneSnapshot = (snapshotName) => {
      const baseMaster = snapshotName.split('@')[0].split('/')[1];
      const defaultCloneName = `tank/${baseMaster}-clone-${Date.now().toString().slice(-4)}`;
      const newMasterName = prompt(`Enter name for the new master ZVOL to be cloned from ${snapshotName}:`, defaultCloneName);
       if (newMasterName) {
          setActionStatus({ message: `Clone Snapshot: Not implemented in backend. Name: ${newMasterName}`, type: 'info' });
          // Placeholder for API call if implemented
          // handleApiAction( ... );
      }
  };






  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-4 md:p-8 font-sans">
      {/* Header */}
      <header className="mb-6 md:mb-8 flex flex-wrap justify-between items-center gap-4">
        <h1 className="text-2xl md:text-3xl font-bold text-gray-800 dark:text-gray-200">Diskless Boot Manager</h1>
        <Button onClick={handleRefresh} variant="outline" size="sm" icon={RefreshCw} disabled={loading}>
          {loading ? 'Refreshing...' : 'Refresh Data'}
        </Button>
      </header>

      {/* Global Error Display */}
      {error && (
          <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded relative mb-6 dark:bg-red-900 dark:border-red-700 dark:text-red-200" role="alert">
              <strong className="font-bold mr-2">Error:</strong>
              <span className="block sm:inline">{error}</span>
          </div>
      )}

       {/* Action Status Display */}
       {actionStatus.message && (
           <div className={`px-4 py-3 rounded relative mb-6 border transition-opacity duration-300 ${
               actionStatus.type === 'error' ? 'bg-red-100 border-red-400 text-red-700 dark:bg-red-900 dark:border-red-700 dark:text-red-200' :
               actionStatus.type === 'success' ? 'bg-green-100 border-green-400 text-green-700 dark:bg-green-900 dark:border-green-700 dark:text-green-200' :
               'bg-blue-100 border-blue-400 text-blue-700 dark:bg-blue-900 dark:border-blue-700 dark:text-blue-200'
           }`} role="alert">
               <span className="block sm:inline">{actionStatus.message}</span>
                <button onClick={() => setActionStatus({ message: '', type: 'info' })} className="absolute top-0 bottom-0 right-0 px-4 py-3">
                  <X className={`h-5 w-5 ${actionStatus.type === 'error' ? 'text-red-500' : actionStatus.type === 'success' ? 'text-green-500': 'text-blue-500'}`}/>
                </button>
           </div>
       )}


      {/* Service Status Cards */}
      <ServiceManagement services={services} refresh={fetchData} loading={loading} />



      {/* Main Content Area */}
      {!loading && !error && (
        <div className="space-y-6 md:space-y-8">

          {/* Client Management */}
          <Card title="Client Management" icon={Users} actions={ // Add button to card actions
                <Button onClick={handleOpenAddClientModal} icon={PlusCircle} disabled={masters.length === 0}>
                    Add Client
                    {masters.length === 0 &&
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
          </Card>

          {/* Master Image Management */}
          <Card title="Master Images & Snapshots" icon={HardDrive} actions={ // Add button to card actions
                <Button onClick={handleOpenCreateMasterModal} icon={PlusCircle}>Create Master</Button>
          }>
            <div className="space-y-6">
              {masters.map((master) => (
                <div key={master.id} className="p-4 border border-gray-200 dark:border-gray-700 rounded-md bg-gray-50 dark:bg-gray-700/50">
                  <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
                    <h4 className="text-lg font-medium break-all">{master.name}</h4>
                    <Button onClick={() => handleCreateSnapshot(master.name)} size="sm" icon={PlusCircle}>Create Snapshot</Button>
                  </div>
                  <h5 className="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400">Available Snapshots:</h5>
                  {master.snapshots && master.snapshots.length > 0 ? ( // Check if snapshots exist
                    <ul className="space-y-2 text-sm">
                      {master.snapshots.map((snap) => (
                        <li key={snap.id || snap.name} className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-gray-100 dark:hover:bg-gray-600/50">
                          <div className="flex-1 min-w-0">
                            <span className="font-mono text-xs break-all">{snap.name}</span>
                            <span className="text-gray-500 dark:text-gray-400 text-xs ml-2 whitespace-nowrap">({snap.created}, {snap.size})</span>
                          </div>
                           <div className="flex space-x-1 flex-shrink-0">
                               <Button onClick={() => handleCloneSnapshot(snap.name)} variant="outline" size="icon" className="h-7 w-7" title={`Clone ${snap.name} to new master`}>
                                  <GitBranchPlus className="h-4 w-4" />
                               </Button>
                               <Button onClick={() => handleDeleteSnapshot(snap.name)} variant="ghost" size="icon" className="h-7 w-7 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/50" title={`Delete ${snap.name}`}>
                                  <Trash2 className="h-4 w-4" />
                               </Button>
                           </div>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-sm text-gray-500 dark:text-gray-400">No snapshots found for this master.</p>
                  )}
                </div>
              ))}
               {masters.length === 0 && !loading && <p className="text-center py-4 text-gray-500">No master images found.</p>}
            </div>
          </Card>
        </div>
      )}

      {/* Loading Indicator */}
      {loading && (
          <div className="fixed inset-0 bg-gray-500 bg-opacity-50 flex items-center justify-center z-[70]">
              <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
          </div>
      )}


      {/* Add Client Modal */}
      <Modal isOpen={isAddClientModalOpen} onClose={() => setIsAddClientModalOpen(false)} title="Add New Client">
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

       {/* Create Master Modal */}
      <Modal isOpen={isCreateMasterModalOpen} onClose={() => setIsCreateMasterModalOpen(false)} title="Create New Master ZVOL">
          <form onSubmit={handleCreateMasterSubmit}>
              <Input
                  label="Master Base Name:" id="masterName" value={newMasterName}
                  onChange={(e) => setNewMasterName(e.target.value)}
                  placeholder="e.g., win11-enterprise (will create pool/name-master)"
                  pattern="^[\w-]+$" title="Use only letters, numbers, underscore, or hyphen" required
              />
              <Input
                  label="Size:" id="masterSize" value={newMasterSize}
                  onChange={(e) => setNewMasterSize(e.target.value)}
                  placeholder="e.g., 50G, 1T"
                  title="Enter size (e.g., 50G, 100G, 1T)" required
              />
               <p className="text-xs text-gray-500 dark:text-gray-400 mt-1 mb-4">
                  This will create a ZFS volume named '{newMasterName ? `${newMasterName}-master` : '...-master'}' in the '{API_BASE_URL.includes('localhost') ? 'tank' : 'configured'}' pool.
               </p>
              <div className="mt-6 flex justify-end space-x-3">
                  <Button type="button" variant="outline" onClick={() => setIsCreateMasterModalOpen(false)}>Cancel</Button>
                  <Button type="submit" icon={Save}>Create Master</Button>
              </div>
          </form>
      </Modal>




      {/* Client Context Menu */}
      <ContextMenu
        isOpen={contextMenu.isOpen}
        xPos={contextMenu.x}
        yPos={contextMenu.y}
        targetClient={contextMenu.client}
        onClose={closeContextMenu}
        actions={clientContextMenuActions}
      />


      <footer className="mt-12 text-center text-sm text-gray-500 dark:text-gray-400">
        Diskless Boot Manager GUI
      </footer>
    </div>
  );
}

export default App;
