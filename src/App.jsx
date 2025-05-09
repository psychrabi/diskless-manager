import {
  RefreshCw
} from 'lucide-react';
import React, { lazy, useCallback, useEffect, useState } from 'react';
import { BrowserRouter as Router, Routes, Route, Link } from 'react-router-dom';

// --- Configuration ---
const API_BASE_URL = 'http://192.168.1.250:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

// --- Helper Functions & Hooks ---


// --- UI Components (Keep existing components: Card, Button, Table, Modal, Input, Select, ContextMenu) ---
const ClientManagement = lazy(() => import('./components/ClientManagement.jsx'));
const ImageManagement = lazy(() => import('./components/ImageManagement.jsx'));
const ServiceManagement = lazy(() => import('./components/ServiceManagement.jsx'));
const Notification = lazy(() => import('./components/ui/Notification.jsx'));

import { Button } from './components/ui/index.js';
import { apiRequest } from './utils/apiRequest.js';




// --- Main Application Component ---
function App() {
  const [clients, setClients] = useState([]);
  const [masters, setMasters] = useState([]);
  const [services, setServices] = useState({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);  
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

      // console.log("Clients:", clientsRes);
      // console.log("Masters:", mastersRes);
      // console.log("Services:", servicesRes);

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
  }, []); // Include selectedSnapshot dependency

  useEffect(() => {
    fetchData();
  }, []); // Run once on mount



  const handleRefresh = () => {
    console.log("Manual refresh triggered.");
    fetchData();
  };

  return (
    <Router>
      <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-2 md:p-4 font-sans">
        {/* Header */}
        <header className="mb-2 md:mb-4 flex flex-wrap justify-between items-center gap-4">
          <div className="flex items-center gap-4">
            <h1 className="text-2xl md:text-3xl font-bold text-gray-800 dark:text-gray-200">Diskless Boot Manager</h1>
            <nav className="hidden md:flex gap-4">
              <Link to="/" className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-gray-100">Dashboard</Link>
              <Link to="/images" className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-gray-100">Master Images</Link>
            </nav>
          </div>
          <Button onClick={handleRefresh} variant="outline" size="sm" icon={RefreshCw} disabled={loading}>
            {loading ? 'Refreshing...' : 'Refresh Data'}
          </Button>
        </header>
        
        {/* Navigation on mobile */}
        <nav className="md:hidden flex justify-center gap-2 mb-4">
          <Link to="/" className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-gray-100">Dashboard</Link>
          <Link to="/images" className="text-gray-600 hover:text-gray-900 dark:text-gray-300 dark:hover:text-gray-100">Master Images</Link>
        </nav>

        {/* Global Error Display */}
        {error && (
            <div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded relative mb-6 dark:bg-red-900 dark:border-red-700 dark:text-red-200" role="alert">
                <strong className="font-bold mr-2">Error:</strong>
                <span className="block sm:inline">{error}</span>
            </div>
        )}

        <Notification />
  {/* Service Status Cards */}
  <ServiceManagement services={services} refresh={fetchData} loading={loading} />
        <Routes>
          <Route path="/" element={
            <div className="space-y-6 md:space-y-8">
            

              {/* Client Management */}
              <ClientManagement clients={clients} masters={masters} fetchData={fetchData} />
            </div>
          } />
          <Route path="/images" element={<ImageManagement masters={masters} fetchData={fetchData} />} />
        </Routes>

        {/* Loading Indicator */}
        {loading && (
          <div className="fixed inset-0 bg-gray-500 bg-opacity-50 flex items-center justify-center z-[70]">
              <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
          </div>
        )}
        
        <footer className="mt-12 text-center text-sm text-gray-500 dark:text-gray-400">
          Diskless Boot Manager GUI
        </footer>
      </div>
    </Router>
  );
}

export default App;
