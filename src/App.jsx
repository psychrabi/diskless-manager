import { invoke } from "@tauri-apps/api/core";
import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { Route, BrowserRouter as Router, Routes } from "react-router-dom";

// --- UI Components (Keep existing components: Card, Button, Table, Modal, Input, Select, ContextMenu) ---
const ClientManagement = lazy(() =>
  import("./components/ClientManagement.jsx")
);
const ImageManagement = lazy(() => import("./components/ImageManagement.jsx"));
const ServiceManagement = lazy(() =>
  import("./components/ServiceManagement.jsx")
);
const Notification = lazy(() => import("./components/ui/Notification.jsx"));
const Sidebar = lazy(() => import("./components/ui/Sidebar.jsx"));
const SetupPage = lazy(() => import("./components/SetupPage.jsx"));

function App() {
  const [clients, setClients] = useState([]);
  const [masters, setMasters] = useState([]);
  const [services, setServices] = useState({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [selectedSnapshot, setSelectedSnapshot] = useState('');
  const [config, setConfig] = useState(null);
  const [checkingConfig, setCheckingConfig] = useState(true);

  // --- Data Fetching ---
  const fetchData = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    setError(null); // Clear previous errors
    try {
      console.log("Fetching data...");
      const [servicesRes, mastersRes, clientsRes] = await Promise.all([
        invoke('get_services', { 'zfsPool': 'diskless' }),
        invoke('get_masters', { 'zfsPool': 'diskless' }),
        invoke('get_clients')
      ]);

      setClients(clientsRes ? Object.values(clientsRes) : []);
      setMasters(mastersRes || []);
      setServices(servicesRes || {});

      // Set default snapshot selection for Add Client modal only if not already set and snapshots exist
      if (!selectedSnapshot && mastersRes?.length > 0 && mastersRes[0].snapshots?.length > 0) {
        setSelectedSnapshot(mastersRes[0].snapshots[mastersRes[0].snapshots.length - 1].name);
      } else if (mastersRes?.flatMap((m) => m.snapshots || []).length === 0) {
        // Clear selection if no snapshots exist anywhere
        setSelectedSnapshot("");
      }
      console.log("Data fetched successfully.");
    } catch (err) {
      console.error("Failed to fetch data:", err);
      setError(`Failed to load data: ${err.message || "Check backend connection."}`);
    } finally {
      if (showLoading) setLoading(false);
    }
  }, []);

   useEffect(() => {
    (async () => {
      
        const cfg = await invoke("get_config");
        setConfig(cfg);
        console.log("Config fetched successfully:", cfg);
     
        setCheckingConfig(false);
        setLoading(false);
        fetchData();
    })();
  }, []);

  //  if (checkingConfig) {
  //   return (
  //     <div className="flex items-center justify-center min-h-screen">
  //       <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
  //     </div>
  //   );
  // }

   if (!config || !config.settings.zpool_name) {
    
    return (
      <Suspense fallback={<div>Loading setup...</div>}>
        <SetupPage />
      </Suspense>
    );
  }

  return (
    <Router>
      <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-2 md:p-4 font-sans">
        <div className="flex">
          <Sidebar />
          <div className="flex-grow ml-56">
            {/* Global Error Display */}
            {error && (
              <div
                className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded relative mb-6 dark:bg-red-900 dark:border-red-700 dark:text-red-200"
                role="alert"
              >
                <strong className="font-bold mr-2">Error:</strong>
                <span className="block sm:inline">{error}</span>
              </div>
            )}

            <Notification />
            {/* Service Status Cards */}

            <Routes>
              <Route path="/" element={<ServiceManagement services={services} refresh={fetchData} />} />
              <Route path="/clients" element={<ClientManagement clients={clients} masters={masters} refresh={fetchData} />} />
              <Route path="/masters" element={<ImageManagement masters={masters} refresh={fetchData} />} />
            </Routes>

            {/* Loading Indicator */}
            {loading && (
              <div className="fixed inset-0 bg-gray-500 bg-opacity-50 flex items-center justify-center z-[70]">
                <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
              </div>
            )}
          </div>
        </div>
      </div>
    </Router>
  );
}

export default App;
