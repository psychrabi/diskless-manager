
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../store/useAppStore';
import { useToastStore } from '../store/useToastStore';

export const useServiceManager = () => {
  const { success, error } = useToastStore()
  // Fetch services is still needed for actions
  const fetchServices = useAppStore(state => state.fetchServices)


  const fetchServiceConfig = async (serviceKey) => {
    try {
      const token = localStorage.getItem('authToken') || '';
      const configData = await invoke('get_service_config', { token, serviceKey });

      let configText = '';
      if (configData && typeof configData === 'object' && 'text' in configData) {
        configText = configData.text;
      } else if (typeof configData === 'object') {
        configText = JSON.stringify(configData, null, 2);
      } else {
        configText = String(configData);
      }
      return { text: configText, path: configData.path };
    } catch (err) {
      error(`Error loading configuration: \n${err.message} `);
    }
  };

  const handleConfigSave = async (serviceKey, content) => {
    try {
      // Get token from localStorage
      const token = localStorage.getItem('authToken') || '';
      await invoke('save_service_config', { token, serviceKey: serviceKey, content: content });
      success('Configuration saved successfully');
      fetchServices();
    } catch (err) {
      error(`Failed to save config: ${err.message || err} `);      
    }
  };

  return {
    fetchServiceConfig,
    handleConfigSave
  };
};
