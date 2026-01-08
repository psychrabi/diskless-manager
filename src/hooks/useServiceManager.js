import * as api from "@/api/commands";
import { useAppStore } from "../store/useAppStore";
import { useToastStore } from "../store/useToastStore";

export const useServiceManager = () => {
  const { success, error } = useToastStore();
  // Fetch services is still needed for actions
  const fetchServices = useAppStore((state) => state.fetchServices);

  const fetchServiceConfig = async (serviceKey) => {
    try {
      const configData = await api.getServiceConfig(serviceKey);

      return { text: configData?.text || "", path: configData?.path || "" };
    } catch (err) {
      error(`Error loading configuration: \n${err.message} `);
      return { text: "", path: "" };
    }
  };

  const handleConfigSave = async (serviceKey, content) => {
    try {
      await api.saveServiceConfig(serviceKey, { content });
      success("Configuration saved successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to save config: ${err.message || err} `);
    }
  };

  const startAllServices = async () => {
    try {
      await api.startAllServices();
      success("All services started successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to start services: ${err.message || err} `);
    }
  };

  const stopAllServices = async () => {
    try {
      await api.stopAllServices();
      success("All services stopped successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to stop services: ${err.message || err} `);
    }
  };
  return {
    fetchServiceConfig,
    handleConfigSave,
    startAllServices,
    stopAllServices,
  };
};
