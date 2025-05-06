import { useState, useCallback } from 'react';
import { validateMacAddress, validateIpAddress } from '../utils/helpers';
import { apiRequest, handleApiAction } from '../utils/apiRequest';

export const useClientManager = (clients, masters, fetchData, showNotification) => {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [newClient, setNewClient] = useState({
    name: '',
    mac: '',
    ip: '',
    master: '',
    snapshot: ''
  });
  
  const validateClient = useCallback((client) => {
    const errors = [];
    if (!client.name.trim()) errors.push('Client name is required');
    if (!validateMacAddress(client.mac)) errors.push('Invalid MAC address format');
    if (!validateIpAddress(client.ip)) errors.push('Invalid IP address format');
    return errors;
  }, []);

  const handleEditClient = useCallback(async (client) => {
    try {
      // TODO: Replace with actual API call
      console.log('Editing client:', client);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      fetchData(); // Refresh data
    } catch (error) {
      console.error('Failed to edit client:', error);
      throw error;
    }
  }, [fetchData]);

  const handleAddNewClientSubmit = async (event) => {
      event.preventDefault();
      // Input validation
      if (!newClient.name.trim()) {
        showNotification('Client name is required.', 'error');
        return;
      }
      
      if (!newClient.mac.trim()) {
        showNotification('MAC address is required.', 'error');
        return;
      }
      
      if (!newClient.ip.trim()) {
        showNotification('IP address is required.', 'error');
        return;
      }
      
      if (!newClient.master) {
        showNotification('Please select a master image.', 'error');
        return;
      }
      
      // Validate MAC address format
      const macRegex = /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/;
      if (!macRegex.test(newClient.mac)) {
        showNotification('Invalid MAC address format. Use XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX', 'error');
        return;
      }
      
      // Validate IP address format
      const ipRegex = /^([\d]{1,3}\.){3}\d{1,3}$/;
      if (!ipRegex.test(newClient.ip)) {
        showNotification('Invalid IP address format. Use X.X.X.X', 'error');
        return;
      }
      console.log(newClient)

      setIsModalOpen(false);
      await handleApiAction(
          () => apiRequest('/clients', 'POST', { 
              name: newClient.name, 
              mac: newClient.mac, 
              ip: newClient.ip, 
              master: newClient.master,
              snapshot: newClient.snapshot ? `${newClient.snapshot}` : null
          }),
          `Client ${newClient.name} added successfully.`,
          `Failed to add client ${newClient.name}`,
          showNotification
      );
    };
  

  const handleAddClient = useCallback(async (client) => {
    try {
      // TODO: Replace with actual API call
      console.log('Adding new client:', client);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      fetchData(); // Refresh data
      setIsModalOpen(false);
    } catch (error) {
      console.error('Failed to add client:', error);
      throw error;
    }
  }, [fetchData, setIsModalOpen]);

  const handleDeleteClient = useCallback(async (client) => {
    try {
      if (!window.confirm(`Are you sure you want to delete client "${client.name}"?`)) return;
      // TODO: Replace with actual API call
      console.log('Deleting client:', client);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      fetchData(); // Refresh data
    } catch (error) {
      console.error('Failed to delete client:', error);
      throw error;
    }
  }, [fetchData]);

  const handleToggleSuperClient = useCallback(async (client) => {
    try {
      // TODO: Replace with actual API call
      console.log('Toggling super client:', client);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      fetchData(); // Refresh data
    } catch (error) {
      console.error('Failed to toggle super client:', error);
      throw error;
    }
  }, [fetchData]);

  return {
    isModalOpen,
    newClient,
    setNewClient,
    setIsModalOpen,
    validateClient,
    handleAddClient,
    handleEditClient,
    handleDeleteClient,
    handleToggleSuperClient,
    handleAddNewClientSubmit,
    masters
  };
};
