import { invoke } from '@tauri-apps/api/core';
import { useCallback } from 'react';
import { useNotification } from '../contexts/NotificationContext';
import { useAppStore } from '../store/useAppStore';

export const useServiceManager = () => {
  const { showNotification } = useNotification();
  const setOpen = useAppStore(state => state.setOpen)
  const setConfig = useAppStore(state => state.setConfig)
  const setTitle = useAppStore(state => state.setTitle)
  const setLoading = useAppStore(state => state.setLoading)
  const setSaving = useAppStore(state => state.setSaving)
  const setServiceKey = useAppStore(state => state.setServiceKey)
  const fetchData = useAppStore(state => state.fetchData)

  const handleServiceAction = useCallback(async (serviceKey, action) => {
    await invoke('control_service', {
      serviceKey: serviceKey,
      req: { action: action }
    }).then((response) => {
      if (response.message) showNotification(response.message, 'success');
    }).catch((error) => showNotification(error, 'error',));
  }, [showNotification]);

  const handleServiceConfigView = useCallback(async (serviceKey, serviceName) => {
    setTitle(`Configuration: ${serviceName}`);
    setOpen(true);
    setLoading(true);
    setServiceKey(serviceKey)
    try {
      const configData = await invoke('get_service_config', { serviceKey });
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
  }, []);

  const handleConfigSave = async (serviceKey, content) => {
    setSaving(true);
    try {
      await invoke('save_service_config', { serviceKey: serviceKey, content: content });
      showNotification('Configuration saved successfully', 'success');
      fetchData();
    } catch (err) {
      showNotification(`Failed to save config: ${err.message || err}`, 'error');
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
