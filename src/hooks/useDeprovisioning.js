import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';


export const useDeprovisioning = () => {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const deprovisionClient = async (mac, options = {}) => {
    setLoading(true);
    setError(null);
    
    try {
      const result = await invoke('deprovision_client', {
        req: {
          mac,
          force: options.force || false,
          keep_zfs: options.keep_zfs || false,
          dry_run: options.dry_run || false,
        }
      });
      
      return { success: true, data: result };
    } catch (err) {
      const errorMsg = err.toString();
      setError(errorMsg);
      return { success: false, error: errorMsg };
    } finally {
      setLoading(false);
    }
  };

  const deprovisionClientById = async (clientId, options = {}) => {
    setLoading(true);
    setError(null);
    
    try {
      const result = await invoke('deprovision_client_by_id', {
        clientId,
        force: options.force || false,
        keep_zfs: options.keep_zfs || false,
      });
      
      return { success: true, data: result };
    } catch (err) {
      const errorMsg = err.toString();
      setError(errorMsg);
      return { success: false, error: errorMsg };
    } finally {
      setLoading(false);
    }
  };

  const getDeprovisionStatus = async (mac) => {
    try {
      const status = await invoke('get_deprovision_status', { mac });
      return { success: true, data: status };
    } catch (err) {
      return { success: false, error: err.toString() };
    }
  };

  const clearError = () => {
    setError(null);
  };

  return {
    deprovisionClient,
    deprovisionClientById,
    getDeprovisionStatus,
    loading,
    error,
    clearError,
  };
};

export default useDeprovisioning;