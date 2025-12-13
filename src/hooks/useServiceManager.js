import { invoke } from '@tauri-apps/api/core';
import { useCallback } from 'react';
import { useAppStore } from '../store/useAppStore';
import { useToastStore } from '../store/useToastStore';

export const useServiceManager = () => {
  const { success, error } = useToastStore()
  const setOpen = useAppStore(state => state.setOpen)
  const setConfig = useAppStore(state => state.setServiceConfig)
  const setTitle = useAppStore(state => state.setTitle)
  const setLoading = useAppStore(state => state.setLoading)
  const setSaving = useAppStore(state => state.setSaving)
  const setServiceKey = useAppStore(state => state.setServiceKey)
  const fetchServices = useAppStore(state => state.fetchServices)

  const handleServiceAction = useCallback(async (serviceKey, action) => {
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('control_service', {
      token,
      serviceKey: serviceKey,
      req: { action: action }
    }).then((response) => {
      if (response.message) success(response.message);
      fetchServices(); // Refresh services status
    }).catch((error) => error(error));
  }, [success, fetchServices]);

  const handleServiceConfigView = useCallback(async (serviceKey, serviceName) => {
    setTitle(`Configuration: ${serviceName}`);
    setOpen(true);
    setLoading(true);
    setServiceKey(serviceKey)
    try {
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      const configData = await invoke('get_service_config', { token, serviceKey });
      if (configData && typeof configData === 'object' && 'text' in configData) {
        setConfig(configData.text);
      } else if (typeof configData === 'object') {
        setConfig(JSON.stringify(configData, null, 2));
      } else {
        setConfig(String(configData));
      }
    } catch (error) {
      setConfig(`Error loading configuration:\n${error.message}`);
    } finally {
      setLoading(false);
    }
  }, [setTitle, setOpen, setLoading, setServiceKey, setConfig]);

  const handleConfigSave = async (serviceKey, content) => {
    setSaving(true);
    try {
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('save_service_config', { token, serviceKey: serviceKey, content: content });
      success('Configuration saved successfully');
      fetchServices();
    } catch (err) {
      error(`Failed to save config: ${err.message || err}`);
    } finally {
      setSaving(false);
      setOpen(false);
    }
  };

  return {
    handleServiceAction,
    handleServiceConfigView,
    handleConfigSave
  };
};
