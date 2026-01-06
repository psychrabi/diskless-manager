import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export const useSettings = () => {
  const [loading, setLoading] = useState(false);
  const { error, success } = useToastStore();
  const fetchConfig = useAppStore((state) => state.fetchConfig);

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
            ...config,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the DHCP service using the new settings
        await invoke("configure_service", { serviceName: "dhcp" });
        await invoke("restart_service", { name: "dhcp" });
        await fetchConfig();
        success("DHCP Settings", "DHCP configuration saved successfully");
        return true;
      } catch (err) {
        console.log(err);
        error(
          "DHCP Settings",
          "Failed to configure DHCP server: " + (err.message ?? err)
        );
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
            ...tftpConfig,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the TFTP service using the new settings
        await invoke("configure_service", { serviceName: "tftp" });
        await invoke("restart_service", { name: "tftp" });
        await fetchConfig();

        success("TFTP Settings", "TFTP configuration saved successfully");
        return true;
      } catch (err) {
        error("TFTP Settings", err.message || String(err));
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error, fetchConfig]
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
            ...httpConfig,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the HTTP service using the new settings
        await invoke("configure_service", { serviceName: "http" });
        await invoke("restart_service", { name: "http" });
        await fetchConfig();

        success("HTTP Settings", "HTTP configuration saved successfully");
        return true;
      } catch (err) {
        error(
          "HTTP Settings",
          err.message || err || "Failed to configure HTTP server"
        );
        return false;
      } finally {
        setLoading(false);
      }
    },
    [error, success, fetchConfig]
  );
  const updateIscsi = useCallback(
    async (iscsiConfig) => {
      setLoading(true);
      try {
        // First, get the current settings
        const currentSettings = await invoke("get_settings");

        // Update the ISCSI settings
        const updatedSettings = {
          ...currentSettings,
          iscsi: {
            ...iscsiConfig,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the ISCSI service using the new settings
        await invoke("configure_service", { serviceName: "iscsi" });
        await invoke("restart_service", { name: "iscsi" });
        await fetchConfig();

        success("ISCSI Settings", "ISCSI configuration saved successfully");
        return true;
      } catch (err) {
        error(
          "ISCSI Settings",
          err.message || err || "Failed to configure ISCSI server"
        );
        return false;
      } finally {
        setLoading(false);
      }
    },
    [error, success, fetchConfig]
  );

  const updateSamba = useCallback(
    async (sambaConfig) => {
      console.log(sambaConfig);
      setLoading(true);
      try {
        // First, get the current settings
        const currentSettings = await invoke("get_settings");

        // Update the Samba settings
        const updatedSettings = {
          ...currentSettings,
          samba: {
            ...sambaConfig,
          },
        };

        // Save the updated settings
        await invoke("save_settings", { settings: updatedSettings });

        // Configure the Samba service using the new settings
        await invoke("configure_service", { serviceName: "samba" });
        await invoke("restart_service", { name: "samba" });
        await fetchConfig();

        success("Samba Settings", "Samba configuration saved successfully");
        return true;
      } catch (err) {
        error("Samba Settings", err.message || String(err));
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error, fetchConfig]
  );
  const updateServer = useCallback(
    async (serverConfig) => {
      setLoading(true);
      try {
        const currentSettings = await invoke("get_settings");
        const updatedSettings = {
          ...currentSettings,
          server: {
            ...serverConfig,
          },
        };
        await invoke("save_settings", { settings: updatedSettings });
        await fetchConfig();
        success("Server Settings", "Server configuration saved successfully");
        return true;
      } catch (err) {
        error("Server Settings", err.message || String(err));
        return false;
      } finally {
        setLoading(false);
      }
    },
    [success, error, fetchConfig]
  );

  const fetchInterfaces = useCallback(async () => {
    try {
      return await invoke("get_network_interfaces");
    } catch (err) {
      error("Network Interfaces", "Failed to fetch network interfaces");
      return [];
    }
  }, [error]);

  const getInterfaceIp = useCallback(async (iface) => {
    try {
      return await invoke("get_interface_ip", { interface: iface });
    } catch (err) {
      console.error("Failed to fetch interface IP:", err);
      return null;
    }
  }, []);
  const detectNetwork = useCallback(async () => {
    try {
      return await invoke("detect_server_network");
    } catch (err) {
      error("Network Detection", "Failed to auto-detect network settings");
      return null;
    }
  }, [error]);
  const applyNetworkSettings = useCallback(async () => {
    setLoading(true);
    try {
      const response = await invoke("apply_network_settings");
      success("Network Applied", response);
      return true;
    } catch (err) {
      error("Network Apply Failed", err.message || String(err));
      return false;
    } finally {
      setLoading(false);
    }
  }, [success, error]);

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
        if (response) success("Admin Password", response.message || response);
        return true;
      } catch (err) {
        error("Admin Password", err.message || "An unknown error occurred");
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
      error("License Info", err?.message || String(err));
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
    updateIscsi,
    updateSamba,
    updateServer,
    fetchInterfaces,
    getInterfaceIp,
    detectNetwork,
    applyNetworkSettings,
    updatePassword,
    getLicenseInfo,
    activateLicense,
  };
};
