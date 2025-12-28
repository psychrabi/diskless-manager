import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store/useAppStore";
import { useToastStore } from "../store/useToastStore";

export const useServiceManager = () => {
  const { success, error } = useToastStore();
  // Fetch services is still needed for actions
  const fetchServices = useAppStore((state) => state.fetchServices);

  const fetchServiceConfig = async (serviceKey) => {
    try {
      const token = localStorage.getItem("authToken") || "";
      const configData = await invoke("get_service_config", {
        token,
        serviceKey,
      });

      // let configText = "";
      // if (
      //   configData &&
      //   typeof configData === "object" &&
      //   "text" in configData
      // ) {
      //   configText = configData.text;
      // } else if (typeof configData === "object") {
      //   configText = JSON.stringify(configData, null, 2);
      // } else {
      //   configText = String(configData);
      // }
      return { text: configData?.text, path: configData.path };
    } catch (err) {
      error(`Error loading configuration: \n${err.message} `);
    }
  };

  const handleConfigSave = async (serviceKey, content) => {
    try {
      // Get token from localStorage
      const token = localStorage.getItem("authToken") || "";
      await invoke("save_service_config", {
        token,
        serviceKey: serviceKey,
        content: content,
      });
      success("Configuration saved successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to save config: ${err.message || err} `);
    }
  };

  const startAllServices = async () => {
    try {
      const token = localStorage.getItem("authToken") || "";
      await invoke("start_all_services", { token });
      success("All services started successfully");
      fetchServices();
    } catch (err) {
      error(`Failed to start services: ${err.message || err} `);
    }
  };

  const stopAllServices = async () => {
    try {
      const token = localStorage.getItem("authToken") || "";
      await invoke("stop_all_services", { token });
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
