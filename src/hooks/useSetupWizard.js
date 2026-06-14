import { useCallback, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/shallow";
import {
  CheckCircle,
  Code,
  Database,
  Globe,
  Network,
  Package,
  Share2,
  Shield,
} from "lucide-react";
import { useServiceManager } from "@/hooks/useServiceManager";
import { useSettings } from "@/hooks/useSettings";
import { useToastStore } from "@/store/useToastStore";
import { useAppStore } from "@/store/useAppStore";
import { listDisks, checkZfsPoolExists, createZfsPool } from "@/api/modules/disks";
import { installService, configureSambaServer } from "@/api/modules/services";

export const getInitialStep = ({
  privilegedAccessGranted,
  allServicesInstalled,
  poolExists,
  hasDhcp,
  hasTftp,
  hasHttp,
  hasSamba,
  hasBootScript,
}) => {
  if (!privilegedAccessGranted) return 1;
  if (
    allServicesInstalled &&
    poolExists &&
    hasDhcp &&
    hasTftp &&
    hasHttp &&
    hasSamba &&
    hasBootScript
  ) {
    return 9;
  }
  if (
    allServicesInstalled &&
    poolExists &&
    hasDhcp &&
    hasTftp &&
    hasHttp &&
    hasSamba
  ) {
    return 8;
  }
  if (allServicesInstalled && poolExists && hasDhcp && hasTftp && hasHttp) {
    return 7;
  }
  if (allServicesInstalled && poolExists && hasDhcp && hasTftp) {
    return 6;
  }
  if (allServicesInstalled && poolExists && hasDhcp) {
    return 5;
  }
  if (allServicesInstalled && poolExists) {
    return 4;
  }
  if (allServicesInstalled) {
    return 3;
  }
  return 2;
};

export const useSetupWizard = () => {
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [installing, setInstalling] = useState("");
  const [activeStep, setActiveStep] = useState(1);
  const [checking, setChecking] = useState(false);
  const [bootScriptContent, setBootScriptContent] = useState(null);
  const [privilegedAccessGranted, setPrivilegedAccessGranted] = useState(false);

  const { appConfig, fetchConfig } = useAppStore();
  const { error, success, info } = useToastStore();
  const { updateDhcp, updateTftp, updateHttp } = useSettings();
  const { handleConfigSave, fetchServiceConfig } = useServiceManager();

  const settings = appConfig?.settings ?? {};
  const poolName = settings.zpool_name || settings.zfsPool || "zroot";
  const hasDhcp = Boolean(settings.dhcp);
  const hasTftp = Boolean(settings.tftp);
  const hasHttp = Boolean(settings.http);
  const hasSamba = Boolean(settings.samba);
  const hasBootScript = Boolean(bootScriptContent);

  const { dependencies, fetchDependencies } = useAppStore(
    useShallow((state) => ({
      dependencies: state.dependencies || [],
      fetchDependencies: state.fetchDependencies,
    }))
  );

  const checkAll = useCallback(async () => {
    setChecking(true);
    try {
      const [detectedDisks, exists] = await Promise.all([
        listDisks(),
        checkZfsPoolExists(),
      ]);
      console.log("Detected disks:", detectedDisks);
      setDisks(detectedDisks);
      setPoolExists(exists);

      await Promise.all([fetchDependencies(), fetchConfig()]);

      try {
        const bootConfig = await fetchServiceConfig("tftp-autoexec");
        if (bootConfig?.text) {
          setBootScriptContent(bootConfig.text);
        }
      } catch (e) {
        console.warn("Failed to fetch boot script:", e);
      }
    } catch (e) {
      console.warn("Initial check failed:", e);
    } finally {
      setChecking(false);
    }
  }, [fetchDependencies, fetchConfig, fetchServiceConfig]);

  useEffect(() => {
    checkAll();
  }, [checkAll]);

  const allServicesInstalled =
    dependencies.length > 0 && !dependencies.some((svc) => !svc.installed);

  useEffect(() => {
    setActiveStep(
      getInitialStep({
        privilegedAccessGranted,
        allServicesInstalled,
        poolExists,
        hasDhcp,
        hasTftp,
        hasHttp,
        hasSamba,
        hasBootScript,
      })
    );
  }, [
    privilegedAccessGranted,
    allServicesInstalled,
    poolExists,
    hasDhcp,
    hasTftp,
    hasHttp,
    hasSamba,
    hasBootScript,
  ]);

  const handleCreatePool = async (data) => {
    try {
      await createZfsPool({
        name: data.name,
        disk: data.disk,
      });
      success("ZFS Setup", `ZFS pool ${data.name} created successfully.`);
      const exists = await checkZfsPoolExists();
      setPoolExists(exists);
    } catch (e) {
      error("Setup Wizard", `Failed to create ZFS pool: ${e}`);
    }
  };

  const handleInstallService = async (service) => {
    setInstalling(service);
    try {
      await installService(service);
      success("Services", `Package ${service} installed successfully.`);
      await fetchDependencies();
    } catch (e) {
      error("Setup Wizard", `Failed to install package: ${e}`);
    } finally {
      setInstalling("");
    }
  };

  const handleSubmitAndAdvance = useCallback(
    async (submit, data, nextStep, title, message) => {
      const ok = await submit(data);
      if (!ok) return;
      setActiveStep(nextStep);
      success(title, message);
    },
    [success]
  );

  const handleDhcpSubmit = useCallback(
    async (data) =>
      handleSubmitAndAdvance(
        updateDhcp,
        data,
        5,
        "Setup - DHCP",
        "DHCP configuration saved successfully"
      ),
    [handleSubmitAndAdvance, updateDhcp]
  );

  const handleTftpSubmit = useCallback(
    async (data) =>
      handleSubmitAndAdvance(
        updateTftp,
        data,
        6,
        "Setup - TFTP",
        "TFTP configuration saved successfully"
      ),
    [handleSubmitAndAdvance, updateTftp]
  );

  const handleHttpSubmit = useCallback(
    async (data) =>
      handleSubmitAndAdvance(
        updateHttp,
        data,
        7,
        "Setup - HTTP",
        "HTTP configuration saved successfully"
      ),
    [handleSubmitAndAdvance, updateHttp]
  );

  const handleSambaSubmit = useCallback(async (shares) => {
    try {
      await configureSambaServer(shares);
      success("Setup - Samba", "Samba configuration saved successfully");
      setActiveStep(8);
    } catch (e) {
      error("Setup Wizard", `Failed to configure Samba: ${e}`);
    }
  }, [success, error]);

  const handleAuthorized = useCallback(() => {
    setPrivilegedAccessGranted(true);
    setActiveStep(2);
  }, []);

  const handleBootScriptSubmit = useCallback(async (content) => {
    info(`Updating Boot Script`);
    try {
      await handleConfigSave("tftp-autoexec", content);
      setBootScriptContent(content);
      setActiveStep(9);
      success("Setup - Boot Script", "Boot Script saved successfully");
    } catch (e) {
      error("Setup Wizard", `Failed to update boot script: ${e}`);
    }
  }, [info, handleConfigSave, success, error]);

  const steps = useMemo(
    () => [
      {
        id: 1,
        title: "Authorize",
        icon: Shield,
        status: privilegedAccessGranted ? "complete" : "current",
      },
      {
        id: 2,
        title: "Dependencies",
        icon: Package,
        status: allServicesInstalled
          ? "complete"
          : activeStep === 2
          ? "current"
          : "upcoming",
      },
      {
        id: 3,
        title: "Storage",
        icon: Database,
        status: poolExists
          ? "complete"
          : activeStep === 3
          ? "current"
          : "upcoming",
      },
      {
        id: 4,
        title: "DHCP",
        icon: Network,
        status: hasDhcp ? "complete" : activeStep === 4 ? "current" : "upcoming",
      },
      {
        id: 5,
        title: "TFTP",
        icon: Network,
        status: hasTftp ? "complete" : activeStep === 5 ? "current" : "upcoming",
      },
      {
        id: 6,
        title: "HTTP",
        icon: Globe,
        status: hasHttp ? "complete" : activeStep === 6 ? "current" : "upcoming",
      },
      {
        id: 7,
        title: "Samba",
        icon: Share2,
        status: hasSamba ? "complete" : activeStep === 7 ? "current" : "upcoming",
      },
      {
        id: 8,
        title: "Boot",
        icon: Code,
        status:
          activeStep > 8
            ? "complete"
            : activeStep === 8
            ? "current"
            : "upcoming",
      },
      {
        id: 9,
        title: "Finished",
        icon: CheckCircle,
        status: activeStep === 9 ? "current" : "upcoming",
      },
    ],
    [
      activeStep,
      privilegedAccessGranted,
      allServicesInstalled,
      poolExists,
      hasDhcp,
      hasTftp,
      hasHttp,
      hasSamba,
    ]
  );

  return {
    activeStep,
    setActiveStep,
    checking,
    installing,
    disks,
    poolExists,
    poolName,
    bootScriptContent,
    privilegedAccessGranted,
    dependencies,
    steps,
    hasDhcp,
    hasTftp,
    hasHttp,
    hasSamba,
    hasBootScript,
    allServicesInstalled,
    appConfig,
    checkAll,
    handleCreatePool,
    handleInstallService,
    handleDhcpSubmit,
    handleTftpSubmit,
    handleHttpSubmit,
    handleSambaSubmit,
    handleAuthorized,
    handleBootScriptSubmit,
  };
};
