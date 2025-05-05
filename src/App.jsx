import {
  RefreshCw,
  X
} from 'lucide-react';
import React, { useCallback, useEffect, useState } from 'react';

// --- Configuration ---
const API_BASE_URL = 'http://192.168.1.209:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

// --- Helper Functions & Hooks ---


// --- UI Components (Keep existing components: Card, Button, Table, Modal, Input, Select, ContextMenu) ---
import ClientManagement from './components/ClientManagement.jsx';
import ImageManagement from './components/ImageManagement.jsx';
import ServiceManagement from './components/ServiceManagement.jsx';
import { Button } from './components/ui/index.js';
import { apiRequest } from './utils/apiRequest.js';




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
  const [selectedSnapshot, setSelectedSnapshot] = useState('');


  // --- Data Fetching ---
  const fetchData = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    setError(null); // Clear previous errors
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

  






  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-2 md:p-4 font-sans">
      {/* Header */}
      <header className="mb-2 md:mb-4 flex flex-wrap justify-between items-center gap-4">
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




      {/* Main Content Area */}
      {!loading && !error && (
        <div className="space-y-6 md:space-y-8">

      {/* Service Status Cards */}
      <ServiceManagement services={services} refresh={fetchData} loading={loading} />

          {/* Client Management */}
         <ClientManagement clients={clients} masters={masters} fetchData={fetchData} />

          {/* Master Image Management */}
          <ImageManagement masters={masters} refresh={fetchData} />
        </div>
      )}

      {/* Loading Indicator */}
      {loading && (
          <div className="fixed inset-0 bg-gray-500 bg-opacity-50 flex items-center justify-center z-[70]">
              <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
          </div>
      )}


      

       {/* Create Master Modal */}
      
      <footer className="mt-12 text-center text-sm text-gray-500 dark:text-gray-400">
        Diskless Boot Manager GUI
      </footer>
    </div>
  );
}

export default App;
