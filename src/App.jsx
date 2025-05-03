import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  HardDrive, Server, Network, Users, Copy, Trash2, PlusCircle, RefreshCw, Power, PowerOff, Edit, Zap, PowerSquare, Sunrise, X, Save, Merge, GitBranchPlus, AlertCircle
} from 'lucide-react';

// --- Configuration ---
const API_BASE_URL = 'http://192.168.1.206:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

// --- Helper Functions & Hooks ---
const useOnClickOutside = (ref, handler) => {
  useEffect(() => {
    const listener = (event) => {
      if (!ref.current || ref.current.contains(event.target)) return;
      handler(event);
    };
    document.addEventListener('mousedown', listener);
    document.addEventListener('touchstart', listener);
    return () => {
      document.removeEventListener('mousedown', listener);
      document.removeEventListener('touchstart', listener);
    };
  }, [ref, handler]);
};

// --- UI Components (Keep existing components: Card, Button, Table, Modal, Input, Select, ContextMenu) ---
const Card = ({ title, icon, children, className = '', titleClassName = '' }) => (
  <div className={`bg-white dark:bg-gray-800 shadow-lg rounded-lg p-4 md:p-6 border border-gray-200 dark:border-gray-700 ${className}`}>
    {title && (
      <div className="flex items-center mb-4">
        {icon && React.createElement(icon, { className: "h-5 w-5 md:h-6 md:w-6 mr-3 text-blue-600 dark:text-blue-400 flex-shrink-0" })}
        <h3 className={`text-lg md:text-xl font-semibold text-gray-800 dark:text-gray-200 ${titleClassName}`}>{title}</h3>
      </div>
    )}
    <div>{children}</div>
  </div>
);

const Button = React.forwardRef(({ children, onClick, variant = 'default', size = 'default', className = '', icon: Icon, disabled = false, title = '', type = 'button' }, ref) => {
  const baseStyle = "inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background";
  const variantStyles = {
    default: "bg-blue-600 text-white hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600",
    destructive: "bg-red-600 text-white hover:bg-red-700 dark:bg-red-500 dark:hover:bg-red-600",
    outline: "border border-gray-300 dark:border-gray-600 bg-transparent hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-800 dark:text-gray-200",
    ghost: "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-800 dark:text-gray-200",
    link: "text-blue-600 dark:text-blue-400 underline-offset-4 hover:underline",
  };
  const sizeStyles = {
    default: "h-10 py-2 px-4",
    sm: "h-9 px-3 rounded-md",
    lg: "h-11 px-8 rounded-md",
    icon: "h-9 w-9 md:h-10 md:w-10",
  };

  return (
    <button
      ref={ref}
      type={type}
      onClick={onClick}
      className={`${baseStyle} ${variantStyles[variant]} ${sizeStyles[size]} ${className}`}
      disabled={disabled}
      title={title || (typeof children === 'string' ? children : '')}
    >
      {Icon && <Icon className={`flex-shrink-0 h-4 w-4 ${size !== 'icon' && children ? 'mr-2' : ''}`} />}
      {size !== 'icon' && children}
    </button>
  );
});
Button.displayName = 'Button';

const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-gray-200 dark:border-gray-700 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-gray-200 dark:border-gray-700 transition-colors hover:bg-gray-100/50 dark:hover:bg-gray-800/50 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-gray-500 dark:text-gray-400 ${className}`}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;

const Modal = ({ isOpen, onClose, title, children }) => {
  if (!isOpen) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-50 backdrop-blur-sm transition-opacity duration-300 ease-in-out">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 w-full max-w-md m-4 border border-gray-200 dark:border-gray-700 transform transition-all duration-300 ease-in-out scale-95 opacity-0 animate-fade-in-scale">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-xl font-semibold">{title}</h2>
          <Button onClick={onClose} variant="ghost" size="icon">
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div>{children}</div>
      </div>
      <style jsx>{`
        @keyframes fade-in-scale {
          from { opacity: 0; transform: scale(0.95); }
          to { opacity: 1; transform: scale(1); }
        }
        .animate-fade-in-scale { animation: fade-in-scale 0.2s ease-out forwards; }
      `}</style>
    </div>
  );
};


const Input = ({ label, id, value, onChange, placeholder, type = "text", required = false, pattern, title }) => (
    <div className="mb-4">
        <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{label}</label>
        <input
            type={type}
            id={id}
            name={id}
            value={value}
            onChange={onChange}
            placeholder={placeholder}
            required={required}
            pattern={pattern}
            title={title}
            className="block w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm focus:ring-blue-500 focus:border-blue-500 sm:text-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100"
        />
    </div>
);

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

const ContextMenu = ({ xPos, yPos, isOpen, onClose, targetClient, actions }) => {
  const menuRef = useRef(null);
  useOnClickOutside(menuRef, onClose);

  if (!isOpen || !targetClient) return null;

  const menuStyle = {
    top: `${yPos}px`,
    left: `${xPos}px`,
  };

  return (
    <div
      ref={menuRef}
      style={menuStyle}
      className="fixed z-[60] bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg py-2 min-w-[180px] animate-fade-in"
    >
      <ul>
        <li><Button onClick={() => { actions.edit(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Edit}>Edit Client</Button></li>
        <li><Button onClick={() => { actions.toggleSuper(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Zap}>{targetClient.isSuperClient ? 'Disable' : 'Enable'} Super Client</Button></li>
        <hr className="my-1 border-gray-200 dark:border-gray-700" />
        <li><Button onClick={() => { actions.reboot(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={RefreshCw}>Reboot</Button></li>
        <li><Button onClick={() => { actions.shutdown(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={PowerSquare}>Shutdown</Button></li>
        <li><Button onClick={() => { actions.wake(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Sunrise}>Wake Up</Button></li>
        <hr className="my-1 border-gray-200 dark:border-gray-700" />
        <li><Button onClick={() => { actions.delete(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/50" icon={Trash2}>Delete Client</Button></li>
      </ul>
       <style jsx>{`
        @keyframes fade-in {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        .animate-fade-in { animation: fade-in 0.1s ease-out forwards; }
      `}</style>
    </div>
  );
};

// --- API Interaction Logic ---
const apiRequest = async (endpoint, method = 'GET', body = null) => {
    const url = `${API_BASE_URL}${endpoint}`;
    const options = {
        method,
        headers: {
            'Content-Type': 'application/json',
            // Add Authorization headers if implementing auth
        },
    };
    if (body) {
        options.body = JSON.stringify(body);
    }

    console.log(`API Request: ${method} ${url}`, body || ''); // Log request

    try {
        const response = await fetch(url, options);
        const responseData = await response.json(); // Attempt to parse JSON regardless of status

        console.log(`API Response: ${response.status} ${url}`, responseData); // Log response

        if (!response.ok) {
            // Throw an error with message from backend if available
            const errorMsg = responseData?.error || responseData?.message || `HTTP error! status: ${response.status}`;
            throw new Error(errorMsg);
        }
        return responseData; // Contains 'message' or actual data
    } catch (error) {
        console.error(`API Error (${method} ${url}):`, error);
        // Rethrow the error so calling function can handle it
        throw error;
    }
};


// --- Main Application Component ---
function App() {
  const [clients, setClients] = useState([]);
  const [masters, setMasters] = useState([]);
  const [services, setServices] = useState({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [actionStatus, setActionStatus] = useState({ message: '', type: 'info' }); // For user feedback

  // Modal State
  const [isAddClientModalOpen, setIsAddClientModalOpen] = useState(false);
  const [newClientName, setNewClientName] = useState('');
  const [newClientMac, setNewClientMac] = useState('');
  const [selectedSnapshot, setSelectedSnapshot] = useState('');

  // Context Menu State
  const [contextMenu, setContextMenu] = useState({ isOpen: false, x: 0, y: 0, client: null });

  // --- Data Fetching ---
  const fetchData = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    setError(null);
    setContextMenu({ isOpen: false, x: 0, y: 0, client: null });
    try {
      console.log("Fetching data...");
      const [clientsRes, mastersRes, servicesRes] = await Promise.all([
          apiRequest('/clients'),
          apiRequest('/masters'),
          apiRequest('/services')
      ]);

      setClients(clientsRes || []); // Ensure array even if API returns null/undefined
      setMasters(mastersRes || []);
      setServices(servicesRes || {});

      // Set default snapshot selection for Add Client modal only if not already set
      if (!selectedSnapshot && mastersRes?.length > 0 && mastersRes[0].snapshots?.length > 0) {
        setSelectedSnapshot(mastersRes[0].snapshots[mastersRes[0].snapshots.length - 1].name);
      }
      console.log("Data fetched successfully.");
    } catch (err) {
      console.error("Failed to fetch data:", err);
      setError(`Failed to load data: ${err.message || 'Check backend connection.'}`);
      // Clear data on error? Or keep stale data? Keeping stale for now.
      // setClients([]); setMasters([]); setServices({});
    } finally {
      if (showLoading) setLoading(false);
    }
  }, [selectedSnapshot]); // Include selectedSnapshot to ensure default is set correctly

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
          fetchData(false); // Refresh data in the background
      } catch (error) {
          setActionStatus({ message: `${errorMessagePrefix}: ${error.message || 'Unknown error'}`, type: 'error' });
      }
  };

  const handleRefresh = () => {
    console.log("Manual refresh triggered.");
    fetchData();
  };

  // --- Client Actions ---
  const handleOpenAddClientModal = () => {
    setNewClientName('');
    setNewClientMac('');
    // Reset snapshot selection to default if needed
    if (masters.length > 0 && masters[0].snapshots?.length > 0) {
         setSelectedSnapshot(masters[0].snapshots[masters[0].snapshots.length - 1].name);
    } else {
        setSelectedSnapshot(''); // No snapshots available
    }
    setIsAddClientModalOpen(true);
  };

  const handleAddNewClientSubmit = async (event) => {
    event.preventDefault();
    if (!selectedSnapshot) {
        setActionStatus({ message: 'Please select a snapshot.', type: 'error' });
        return;
    }
    setIsAddClientModalOpen(false); // Close modal immediately
    await handleApiAction(
        () => apiRequest('/clients', 'POST', { name: newClientName, mac: newClientMac, snapshot: selectedSnapshot }),
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
        setActionStatus({ message: `Edit Client ${client.name}: Not implemented.`, type: 'info' });
        handleApiAction(
            () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'edit' }), // Conceptual endpoint
            `Edit action triggered for ${client.name}.`,
            `Failed to trigger edit for ${client.name}`
        );
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
  const handleCreateSnapshot = (masterName) => {
      const snapSuffix = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-'); // YYYY-MM-DD-HH-MM
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
       // Encode the snapshot name for the URL path
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
      const baseMaster = snapshotName.split('@')[0].split('/')[1]; // Extract master name part
      const defaultCloneName = `tank/${baseMaster}-clone-${Date.now().toString().slice(-4)}`; // Add timestamp part
      const newMasterName = prompt(`Enter name for the new master ZVOL to be cloned from ${snapshotName}:`, defaultCloneName);
       if (newMasterName) {
          setActionStatus({ message: `Clone Snapshot: Not implemented in backend. Name: ${newMasterName}`, type: 'info' });
          // Placeholder for API call if implemented
          // handleApiAction(
          //    () => apiRequest('/masters', 'POST', { action: 'clone', sourceSnapshot: snapshotName, newMasterName: newMasterName }),
          //    `Snapshot ${snapshotName} cloned to ${newMasterName}.`,
          //    `Failed to clone snapshot ${snapshotName}`
          // );
      }
  };


  // --- Service Actions ---
  const handleServiceAction = (serviceKey, action) => {
       setActionStatus({ message: `Service Action: ${action} for ${serviceKey} not implemented in backend.`, type: 'info' });
        // Placeholder for API call if implemented
        // handleApiAction(
        //     () => apiRequest(`/services/${serviceKey}/control`, 'POST', { action: action }),
        //     `Action ${action} sent to service ${serviceKey}.`,
        //     `Failed to send action ${action} to service ${serviceKey}`
        // );
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
           <div className={`px-4 py-3 rounded relative mb-6 border ${
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
       <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6 md:mb-8">
          {Object.entries(services).length > 0 ? Object.entries(services).map(([key, service]) => (
              <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg truncate" >
                  <div className="flex items-center justify-between">
                       <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${
                           service.status === 'active' || service.status === 'running' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
                           service.status === 'inactive' || service.status === 'stopped' ? 'bg-gray-200 text-gray-800 dark:bg-gray-700 dark:text-gray-300' :
                           'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200' // error, failed, degraded etc.
                       }`}>
                          {service.status}
                      </span>
                      <div className="flex space-x-1">
                          <Button onClick={() => handleServiceAction(key, 'restart')} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.name}`}>
                              <RefreshCw className="h-4 w-4 text-blue-500" />
                          </Button>
                      </div>
                  </div>
              </Card>
          )) : !loading && <p className="text-gray-500 col-span-full">Could not load service status.</p> }
       </div>


      {/* Main Content Area */}
      {!loading && !error && (
        <div className="space-y-6 md:space-y-8">

          {/* Client Management */}
          <Card title="Client Management" icon={Users}>
            <div className="mb-4 flex justify-end">
              <Button onClick={handleOpenAddClientModal} icon={PlusCircle} disabled={masters.length === 0 || masters.every(m => m.snapshots.length === 0)}>
                  Add New Client
              </Button>
               {(masters.length === 0 || masters.every(m => m.snapshots.length === 0)) &&
                  <span className="text-xs text-red-500 ml-2 self-center">(Requires at least one snapshot)</span>}
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead className="hidden md:table-cell">MAC Address</TableHead>
                  <TableHead>IP Address</TableHead>
                  <TableHead className="hidden xl:table-cell">ZFS Clone</TableHead>
                  {/* <TableHead className="hidden 2xl:table-cell">iSCSI Target</TableHead> */}
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
                    {/* <TableCell className="hidden 2xl:table-cell text-xs font-mono break-all">{client.target}</TableCell> */}
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
          <Card title="Master Images & Snapshots" icon={HardDrive}>
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
          <form onSubmit={handleAddNewClientSubmit}>
              <Input
                  label="Client Name:"
                  id="clientName"
                  value={newClientName}
                  onChange={(e) => setNewClientName(e.target.value)}
                  placeholder="e.g., Workstation-05 (alphanumeric, -, _)"
                  pattern="^[\w-]+$" // Allow alphanumeric, underscore, hyphen
                  title="Use only letters, numbers, underscore, or hyphen"
                  required
              />
              <Input
                  label="MAC Address:"
                  id="macAddress"
                  value={newClientMac}
                  onChange={(e) => setNewClientMac(e.target.value.toUpperCase())}
                  placeholder="00:1A:2B:3C:4D:5E"
                  pattern="^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$" // Basic MAC format validation
                  title="Enter MAC address in format XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX"
                  required
              />
              <Select
                  label="Clone from Snapshot:"
                  id="snapshot"
                  value={selectedSnapshot}
                  onChange={(e) => setSelectedSnapshot(e.target.value)}
                  required
              >
                  <option value="" disabled>-- Select Snapshot --</option>
                  {masters.flatMap(master => master.snapshots || []).map(snap => ( // Add safety check for snapshots array
                       <option key={snap.id || snap.name} value={snap.name}>{snap.name}</option>
                  ))}
              </Select>
              <div className="mt-6 flex justify-end space-x-3">
                  <Button type="button" variant="outline" onClick={() => setIsAddClientModalOpen(false)}>Cancel</Button>
                  <Button type="submit" icon={Save}>Add Client</Button>
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
