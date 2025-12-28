import { useToastStore } from "@/store/useToastStore";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export const useSettings = () => {
  const [loading, setLoading] = useState(false);
  const { error, success } = useToastStore();

  const readConfig = useCallback(async () => {
    try {
      return await invoke("read_config");
    } catch (error) {
      console.error("Failed to load config:", error);
      return null;
    }
  }, []);

  const updateDhcp = useCallback(
    async (config) => {
      setLoading(true);
      try {
        // First, get the current settings
        const currentSettings = await invoke("get_settings");

        // Update the DHCP settings
        const updatedSettings = {
          ...currentSettings,
          dhcp: {
            ...currentSettings.dhcp,
            start_ip: config.start_ip,
            end_ip: config.end_ip,
            subnet_mask: config.subnet_mask,
            gateway_ip: config.gateway_ip,
            dns_server1: config.dns_server1,
            dns_server2: config.dns_server2,
            enabled: config.enabled,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the DHCP service using the new settings
        await invoke("configure_service", { serviceName: "dhcp" });

        success("DHCP configuration saved successfully", "success");
        return true;
      } catch (err) {
        console.log(err);
        error("Failed to configure DHCP server: " + (err.message ?? err));
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error]
  );

  const updateTftp = useCallback(
    async (tftpConfig) => {
      setLoading(true);
      try {
        // First, get the current settings
        const currentSettings = await invoke("get_settings");

        // Update the TFTP settings
        const updatedSettings = {
          ...currentSettings,
          tftp: {
            ...currentSettings.tftp,
            root_dir: tftpConfig.root_dir,
            enabled: true, // Enable TFTP service
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the TFTP service using the new settings
        await invoke("configure_service", { serviceName: "tftp" });

        success("TFTP configuration saved successfully", "success");
        return true;
      } catch (err) {
        error(err);
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error]
  );

  const updateHttp = useCallback(
    async (httpConfig) => {
      setLoading(true);
      try {
        // First, get the current settings
        const currentSettings = await invoke("get_settings");

        // Update the HTTP settings
        const updatedSettings = {
          ...currentSettings,
          http: {
            ...currentSettings.http,
            root_dir: httpConfig.root_dir,
            enabled: true, // Enable HTTP service
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the HTTP service using the new settings
        await invoke("configure_service", { serviceName: "http" });

        success("HTTP configuration saved successfully", "success");
        return true;
      } catch (err) {
        error(
          "Failed to configure HTTP server",
          err.message || "An unknown error occurred"
        );
        return false;
      } finally {
        setLoading(false);
      }
    },
    [error, success]
  );

  const updatePassword = useCallback(
    async (oldPassword, newPassword) => {
      setLoading(true);
      const token = localStorage.getItem("authToken") || "";
      try {
        const response = await invoke("update_admin_password", {
          token,
          oldPassword,
          newPassword,
        });
        if (response) success(response);
        return true;
      } catch (err) {
        error(
          "Failed to update admin password",
          err.message || "An unknown error occurred"
        );
        return false;
      } finally {
        setLoading(false);
      }
    },
    [error, success]
  );

  const getLicenseInfo = useCallback(async () => {
    try {
      return await invoke("get_license_info");
    } catch (err) {
      error("Failed to load license info", err?.message || String(err));
      return null;
    }
  }, [error]);

  const activateLicense = useCallback(
    async (key) => {
      setLoading(true);
      try {
        const resp = await invoke("activate_license", { key });
        success(
          "License Activated",
          resp?.message || "License activated successfully"
        );
        return true;
      } catch (err) {
        error("License Activation Failed", err?.message || String(err));
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error]
  );

  return {
    loading,
    readConfig,
    updateDhcp,
    updateTftp,
    updateHttp,
    updatePassword,
    getLicenseInfo,
    activateLicense,
  };
};
