import { getSettings, saveSettings } from "@/api/modules/system";
import { configureService, restartService } from "@/api/modules/services";
import { readConfig as readConfigApi } from "@/api/modules/config";
import { getNetworkInterfaces, getInterfaceIp as getInterfaceIpApi, detectServerNetwork, applyNetworkSettings as applyNetworkSettingsApi } from "@/api/modules/network";
import { updateAdminPassword } from "@/api/modules/auth";
import { getLicenseInfo as getLicenseInfoApi, activateLicense as activateLicenseApi } from "@/api/modules/license";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useCallback, useState } from "react";

export const useSettings = () => {
  const [loading, setLoading] = useState(false);
  const { error, success } = useToastStore();
  const fetchConfig = useAppStore((state) => state.fetchConfig);

  const withLoading = useCallback(async (task) => {
    setLoading(true);
    try {
      return await task();
    } finally {
      setLoading(false);
    }
  }, []);

  const updateSettingsSection = useCallback(
    async ({
      section,
      config,
      toastTitle,
      successMessage,
      serviceName,
      getErrorMessage = (err) => err?.message || String(err),
    }) =>
      withLoading(async () => {
        try {
          const currentSettings = await getSettings();
          const updatedSettings = {
            ...currentSettings,
            [section]: {
              ...config,
            },
          };

          await saveSettings(updatedSettings);

          if (serviceName) {
            await configureService(serviceName);
            await restartService(serviceName);
          }

          await fetchConfig();
          success(toastTitle, successMessage);
          return true;
        } catch (err) {
          error(toastTitle, getErrorMessage(err));
          return false;
        }
      }),
    [error, fetchConfig, success, withLoading]
  );

  const readConfig = useCallback(async () => {
    try {
      return await readConfigApi();
    } catch (err) {
      console.error("Failed to load config:", err);
      return null;
    }
  }, []);

  const updateDhcp = useCallback(
    (config) =>
      updateSettingsSection({
        section: "dhcp",
        config,
        toastTitle: "DHCP Settings",
        successMessage: "DHCP configuration saved successfully",
        serviceName: "dhcp",
        getErrorMessage: (err) =>
          `Failed to configure DHCP server: ${err?.message ?? err}`,
      }),
    [updateSettingsSection]
  );

  const updateTftp = useCallback(
    (tftpConfig) =>
      updateSettingsSection({
        section: "tftp",
        config: tftpConfig,
        toastTitle: "TFTP Settings",
        successMessage: "TFTP configuration saved successfully",
        serviceName: "tftp",
      }),
    [updateSettingsSection]
  );

  const updateHttp = useCallback(
    (httpConfig) =>
      updateSettingsSection({
        section: "http",
        config: httpConfig,
        toastTitle: "HTTP Settings",
        successMessage: "HTTP configuration saved successfully",
        serviceName: "http",
        getErrorMessage: (err) =>
          err?.message || err || "Failed to configure HTTP server",
      }),
    [updateSettingsSection]
  );
  const updateIscsi = useCallback(
    (iscsiConfig) =>
      updateSettingsSection({
        section: "iscsi",
        config: iscsiConfig,
        toastTitle: "ISCSI Settings",
        successMessage: "ISCSI configuration saved successfully",
        serviceName: "iscsi",
        getErrorMessage: (err) =>
          err?.message || err || "Failed to configure ISCSI server",
      }),
    [updateSettingsSection]
  );

  const updateSamba = useCallback(
    (sambaConfig) =>
      updateSettingsSection({
        section: "samba",
        config: sambaConfig,
        toastTitle: "Samba Settings",
        successMessage: "Samba configuration saved successfully",
        serviceName: "samba",
      }),
    [updateSettingsSection]
  );
  const updateServer = useCallback(
    (serverConfig) =>
      updateSettingsSection({
        section: "server",
        config: serverConfig,
        toastTitle: "Server Settings",
        successMessage: "Server configuration saved successfully",
      }),
    [updateSettingsSection]
  );

  const fetchInterfaces = useCallback(async () => {
    try {
      return await getNetworkInterfaces();
    } catch {
      error("Network Interfaces", "Failed to fetch network interfaces");
      return [];
    }
  }, [error]);

  const getInterfaceIp = useCallback(async (iface) => {
    try {
      return await getInterfaceIpApi(iface);
    } catch (err) {
      console.error("Failed to fetch interface IP:", err);
      return null;
    }
  }, []);
  const detectNetwork = useCallback(async () => {
    try {
      return await detectServerNetwork();
    } catch {
      error("Network Detection", "Failed to auto-detect network settings");
      return null;
    }
  }, [error]);
  const applyNetworkSettings = useCallback(async () => {
    return withLoading(async () => {
      try {
        const response = await applyNetworkSettingsApi({});
        success("Network Applied", response);
        return true;
      } catch (err) {
        error("Network Apply Failed", err.message || String(err));
        return false;
      }
    });
  }, [error, success, withLoading]);

  const updatePassword = useCallback(
    (oldPassword, newPassword) =>
      withLoading(async () => {
        try {
          const response = await updateAdminPassword({
            old_password: oldPassword,
            new_password: newPassword,
          });
          if (response) success("Admin Password", response.message || response);
          return true;
        } catch (err) {
          error("Admin Password", err.message || "An unknown error occurred");
          return false;
        }
      }),
    [error, success, withLoading]
  );

  const getLicenseInfo = useCallback(async () => {
    try {
      return await getLicenseInfoApi();
    } catch (err) {
      error("License Info", err?.message || String(err));
      return null;
    }
  }, [error]);

  const activateLicense = useCallback(
    (key) =>
      withLoading(async () => {
        try {
          const resp = await activateLicenseApi(key);
          success(
            "License Activated",
            resp?.message || "License activated successfully"
          );
          return true;
        } catch (err) {
          error("License Activation Failed", err?.message || String(err));
          return false;
        }
      }),
    [error, success, withLoading]
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
