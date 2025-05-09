import { useState, useCallback, useEffect } from 'react';
import { apiRequest } from '../utils/apiRequest.js';

export const useDataFetching = () => {
  const [clients, setClients] = useState([]);
  const [masters, setMasters] = useState([]);
  const [services, setServices] = useState({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [selectedSnapshot, setSelectedSnapshot] = useState('');

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
  }, [selectedSnapshot]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return {
    clients,
    masters,
    services,
    loading,
    error,
    selectedSnapshot,
    setSelectedSnapshot,
    fetchData
  };
}; 