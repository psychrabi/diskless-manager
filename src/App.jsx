import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  HardDrive, Server, Network, Users, Copy, Trash2, PlusCircle, RefreshCw, Power, PowerOff, Edit, Zap, PowerSquare, Sunrise, X, Save, Merge, GitBranchPlus, AlertCircle, FileText, Eye // Added icons
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
const Card = ({ title, icon, children, className = '', titleClassName = '', actions }) => ( // Added actions prop
  <div className={`bg-white dark:bg-gray-800 shadow-lg rounded-lg p-4 md:p-6 border border-gray-200 dark:border-gray-700 flex flex-col ${className}`}>
    {title && (
      <div className="flex justify-between items-center mb-4">
        <div className="flex items-center min-w-0"> {/* Ensure title area doesn't overflow */}
            {icon && React.createElement(icon, { className: "h-5 w-5 md:h-6 md:w-6 mr-3 text-blue-600 dark:text-blue-400 flex-shrink-0" })}
            <h3 className={`text-lg md:text-xl font-semibold text-gray-800 dark:text-gray-200 truncate ${titleClassName}`}>{title}</h3>
        </div>
        {actions && <div className="flex space-x-2 flex-shrink-0">{actions}</div>} {/* Render actions if provided */}
      </div>
    )}
    <div className="flex-grow">{children}</div> {/* Allow content to grow */}
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

const Modal = ({ isOpen, onClose, title, children, size = 'md' }) => { // Added size prop
  if (!isOpen) return null;

  const sizeClasses = {
      sm: 'max-w-sm',
      md: 'max-w-md',
      lg: 'max-w-lg',
      xl: 'max-w-xl',
      '2xl': 'max-w-2xl',
      full: 'max-w-full h-full m-0 rounded-none', // Example for full screen
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-50 backdrop-blur-sm transition-opacity duration-300 ease-in-out">
      <div className={`bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 w-full m-4 border border-gray-200 dark:border-gray-700 transform transition-all duration-300 ease-in-out scale-95 opacity-0 animate-fade-in-scale ${sizeClasses[size]}`}>
        <div className="flex justify-between items-center mb-4 border-b border-gray-200 dark:border-gray-700 pb-3">
          <h2 className="text-xl font-semibold">{title}</h2>
          <Button onClick={onClose} variant="ghost" size="icon" className="h-8 w-8">
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div className={size === 'full' ? 'overflow-auto h-[calc(100%-80px)]' : ''}>{children}</div> {/* Adjust height for full size */}
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
const apiRequest = async (endpoint, method = 'GET', body = null, responseType = 'json') => {
    const url = `${API_BASE_URL}${endpoint}`;
    const options = {
        method,
        headers: {
            // Only set Content-Type if there's a body
            ...(body && { 'Content-Type': 'application/json' }),
            // Add Authorization headers if implementing auth
        },
    };
    if (body) {
        options.body = JSON.stringify(body);
    }

    console.log(`API Request: ${method} ${url}`, body || ''); // Log request

    try {
        const response = await fetch(url, options);

        // Handle different response types
        let responseData;
        if (responseType === 'text') {
            responseData = await response.text();
        } else { // Default to json
            responseData = await response.json();
        }

        console.log(`API Response: ${response.status} ${url}`, responseType === 'json' ? responseData : '(text response)'); // Log response

        if (!response.ok) {
            // Try to get error message from JSON body, otherwise use status text
            const errorMsg = (responseType === 'json' && responseData?.error)
                             || (responseType === 'json' && responseData?.message)
                             || response.statusText
                             || `HTTP error! status: ${response.status}`;
            throw new Error(errorMsg);
        }
        return responseData; // Contains 'message' or actual data (JSON or text)
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

  const [isViewConfigModalOpen, setIsViewConfigModalOpen] = useState(false); // New state for view config modal
  const [configContent, setConfigContent] = useState('');
  const [configTitle, setConfigTitle] = useState('');
  const [configLoading, setConfigLoading] = useState(false);

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
    setNewClientName('pc_01');
    setNewClientMac('d8:43:ae:a7:8e:a7');
    setNewClientIp('192.168.1.1');
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
        setActionStatus({ message: `Edit Client ${client.name}: Not implemented.`, type: 'info' });
        // Placeholder API call if endpoint exists
        // handleApiAction(
        //     () => apiRequest(`/clients/${client.id}/control`, 'POST', { action: 'edit' }),
        //     `Edit action triggered for ${client.name}.`,
        //     `Failed to trigger edit for ${client.name}`
        // );
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


  // --- Service Actions ---
  const handleServiceRestart = (serviceKey) => {
       handleApiAction(
            () => apiRequest(`/services/${serviceKey}/control`, 'POST', { action: 'restart' }),
            `Restart command sent to service ${serviceKey}.`,
            `Failed to send restart command to service ${serviceKey}`
        );
  };

  const handleViewConfig = async (serviceKey, serviceName) => {
      setConfigTitle(`Configuration: ${serviceName}`);
      setConfigContent(''); // Clear previous content
      setIsViewConfigModalOpen(true);
      setConfigLoading(true);
      try {
          // Use responseType 'text' for config files
          const configData = await apiRequest(`/services/${serviceKey}/config`, 'GET', null, 'text');
          setConfigContent(configData);
      } catch (error) {
          setConfigContent(`Error loading configuration:\n${error.message}`);
      } finally {
          setConfigLoading(false);
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
       <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6 md:mb-8">
          {Object.entries(services).length > 0 ? Object.entries(services).map(([key, service]) => (
              <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
                  <div className="flex items-center justify-between">
                       <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${
                           service.status === 'active' || service.status === 'running' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
                           service.status === 'inactive' || service.status === 'stopped' ? 'bg-gray-200 text-gray-800 dark:bg-gray-700 dark:text-gray-300' :
                           'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200' // error, failed, degraded etc.
                       }`}>
                          {service.status}
                      </span>
                      {/* Service Action Buttons */}
                      <div className="flex space-x-1">
                          <Button onClick={() => handleViewConfig(key, service.name)} variant="ghost" size="icon" className="h-7 w-7" title={`View Config for ${service.name}`}>
                              <Eye className="h-4 w-4 text-gray-500" />
                          </Button>
                          <Button onClick={() => handleServiceRestart(key)} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.name}`}>
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

       {/* View Config Modal */}
       <Modal isOpen={isViewConfigModalOpen} onClose={() => setIsViewConfigModalOpen(false)} title={configTitle} size="2xl">
            {configLoading ? (
                <div className="flex justify-center items-center h-40">
                    <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
                </div>
            ) : (
                <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-md text-xs overflow-auto max-h-[70vh]">
                    <code>{configContent}</code>
                </pre>
            )}
             <div className="mt-4 flex justify-end">
                  <Button variant="outline" onClick={() => setIsViewConfigModalOpen(false)}>Close</Button>
              </div>
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
