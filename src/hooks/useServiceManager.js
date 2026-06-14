import { useCallback } from "react";
import { getServiceConfig, saveServiceConfig, startAllServices as startAllServicesApi, stopAllServices as stopAllServicesApi } from "@/api/modules/services";
import { useAppStore } from "../store/useAppStore";
import { useToastStore } from "../store/useToastStore";

export const useServiceManager = () => {
  const { success, error } = useToastStore();
  const fetchServices = useAppStore((state) => state.fetchServices);

  const fetchServiceConfig = useCallback(async (serviceKey) => {
    try {
      const configData = await getServiceConfig(serviceKey);

      return { text: configData?.text || "", path: configData?.path || "" };
    } catch (err) {
      error(`Error loading configuration: \n${err.message} `);
      return { text: "", path: "" };
    }
  }, [error]);

  const handleConfigSave = useCallback(async (serviceKey, content) => {
    try {
      await saveServiceConfig(serviceKey, { content });
      success("Configuration saved successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to save config: ${err.message || err} `);
    }
  }, [success, error, fetchServices]);

  const startAllServices = useCallback(async () => {
    try {
      await startAllServicesApi();
      success("All services started successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to start services: ${err.message || err} `);
    }
  }, [success, error, fetchServices]);

  const stopAllServices = useCallback(async () => {
    try {
      await stopAllServicesApi();
      success("All services stopped successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to stop services: ${err.message || err} `);
    }
  }, [success, error, fetchServices]);
  return {
    fetchServiceConfig,
    handleConfigSave,
    startAllServices,
    stopAllServices,
  };
};
