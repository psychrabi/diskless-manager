import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  HardDrive, Server, Network, Users, Copy, Trash2, PlusCircle, RefreshCw, Power, PowerOff, Edit, Zap, PowerSquare, Sunrise, X, Save, GitBranchPlus, Archive, Trash
} from 'lucide-react';
import { useDataFetcher } from './hooks/useDataFetcher';
import { useClientManager } from './hooks/useClientManager';
import { useMasterManager } from './hooks/useMasterManager';
import { useServiceManager } from './hooks/useServiceManager';
import { useOnClickOutside } from './hooks/useOnClickOutside';
import { Card, Button, Modal, Input, Select, Table, ContextMenu } from './components/ui/index.js';
import { ImageManagement } from './components/ImageManagement';
import { ClientManagement } from './components/ClientManagement';
import { ServiceManagement } from './components/ServiceManagement';

// --- Main Application Component ---
function App() {
  const { data, loading, error, fetchData } = useDataFetcher();
  const { clients, masters, services } = data;

  const [contextMenu, setContextMenu] = useState({ isOpen: false, x: 0, y: 0, client: null });
  const menuRef = useRef(null);

  const handleRefresh = useCallback(() => {
    fetchData();
  }, [fetchData]);



  const handleClientContextMenu = useCallback((event, client) => {
    event.preventDefault();
    setContextMenu({
      isOpen: true,
      x: event.clientX,
      y: event.clientY,
      client: client,
    });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu(prev => ({ ...prev, isOpen: false }));
  }, []);

  const handleAddClient = useCallback((client) => {
    // TODO: Implement actual client addition logic
    return new Promise((resolve) => setTimeout(resolve, 500));
  }, []);
  const clientContextMenuActions = {
    edit: () => alert('Edit Client'),
    toggleSuper: () => alert('Toggle Super Client'),
    reboot: (client) => {
      alert(`Reboot Client: ${client.name}`);
    },
    shutdown: (client) => {
      fetchData();
      alert(`Shutdown Client: ${client.name}`);
    },
    wake: (client) => {
      fetchData();
      alert(`Wake Up Client: ${client.name}`);
    },
    delete: () => alert('Delete Client'),
  };

  // Hook Functions


  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-4 md:p-8 font-sans">
      <header className="mb-6 md:mb-8 flex flex-wrap justify-between items-center gap-4">
        <h1 className="text-2xl md:text-3xl font-bold text-gray-800 dark:text-gray-200">Diskless Boot Manager</h1>
        <Button onClick={handleRefresh} variant="outline" size="sm" icon={RefreshCw} disabled={loading}>
          {loading ? 'Refreshing...' : 'Refresh Data'}
        </Button>
      </header>

      {error && <p className="text-red-500 bg-red-100 dark:bg-red-900 p-4 rounded-md mb-6">{error}</p>}

      {!loading && !error && (
        <div className="space-y-6 md:space-y-8">
          <ServiceManagement services={services} refresh={fetchData} loading={loading} />
          <ClientManagement clients={clients} masters={masters} fetchData={fetchData} />
          <ImageManagement masters={masters} fetchData={fetchData} />
        </div>
      )}


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
        Diskless Boot Manager GUI - Enhanced Frontend
      </footer>
    </div>
  );
}

export default App;
