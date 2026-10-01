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
import { listDisks, createZfsPool } from "@/api/modules/disks";
import { installService } from "@/api/modules/services";
import { getSetupStatus, completeSetup } from "@/api/modules/system";

const getInitialStep = ({
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
  const [setupStatus, setSetupStatus] = useState(null);
  const [setupError, setSetupError] = useState("");
  const [completing, setCompleting] = useState(false);
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [installing, setInstalling] = useState("");
  const [activeStep, setActiveStep] = useState(1);
  const [checking, setChecking] = useState(false);
  const [privilegedAccessGranted, setPrivilegedAccessGranted] = useState(false);

  const { appConfig, fetchConfig } = useAppStore();
  const { error, success, info } = useToastStore();
  const { updateDhcp, updateTftp, updateHttp, updateSamba } = useSettings();
  const { handleConfigSave } = useServiceManager();

  const settings = appConfig?.settings ?? {};
  const poolName = settings.zpool_name || settings.zfsPool || "diskless";
  const has = (key) => Boolean(setupStatus && !setupStatus.missing.includes(key));
  const hasDhcp = has("dhcp") && has("settings");
  const hasTftp = has("tftp");
  const hasHttp = has("http");
  const hasSamba = has("samba");
  const hasBootScript = has("boot");

  const { dependencies, fetchDependencies } = useAppStore(
    useShallow((state) => ({
      dependencies: state.dependencies || [],
      fetchDependencies: state.fetchDependencies,
    }))
  );

  const checkAll = useCallback(async () => {
    setChecking(true);
    try {
      setSetupError("");
      const status = await getSetupStatus();
      setSetupStatus(status);
      setPrivilegedAccessGranted(!status.missing.includes("authorization"));
      const detectedDisks = status.missing.includes("authorization") ? [] : await listDisks();
      setDisks(detectedDisks);
      setPoolExists(!status.missing.includes("storage"));

      await Promise.all([fetchDependencies(), fetchConfig()]);

    } catch (e) {
      setSetupStatus(null);
      setSetupError(e.message || "Unable to check server setup");
    } finally {
      setChecking(false);
    }
  }, [fetchDependencies, fetchConfig]);

  useEffect(() => {
    // Defer so setState inside checkAll() is not synchronous within
    // the effect body (react-hooks/set-state-in-effect).
    const timer = setTimeout(checkAll, 0);
    return () => clearTimeout(timer);
  }, [checkAll]);

  const allServicesInstalled = has("dependencies");

  useEffect(() => {
    // Defer so setActiveStep is not synchronous within the effect
    // body (react-hooks/set-state-in-effect).
    const timer = setTimeout(() => {
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
    }, 0);
    return () => clearTimeout(timer);
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
      const result = await createZfsPool({
        name: data.name,
        disk: data.disk,
      });
      if (!result.success) throw new Error(result.message || "Pool creation failed");
      success("ZFS Setup", `ZFS pool ${data.name} created successfully.`);
      await checkAll();
    } catch (e) {
      error("Setup Wizard", `Failed to create ZFS pool: ${e}`);
    }
  };

  const handleInstallService = async (service) => {
    setInstalling(service);
    try {
      await installService(service);
      success("Services", `Package ${service} installed successfully.`);
      await checkAll();
    } catch (e) {
      error("Setup Wizard", `Failed to install package: ${e}`);
    } finally {
      setInstalling("");
    }
  };

  const handleSubmitAndAdvance = useCallback(
    async (submit, data, title, message) => {
      const ok = await submit(data);
      if (!ok) return;
      await checkAll();
      success(title, message);
    },
    [success, checkAll]
  );

  const handleDhcpSubmit = useCallback(
    async (data) =>
      handleSubmitAndAdvance(
        updateDhcp,
        data,
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
        "Setup - HTTP",
        "HTTP configuration saved successfully"
      ),
    [handleSubmitAndAdvance, updateHttp]
  );

  const handleSambaSubmit = useCallback(
    (data) => handleSubmitAndAdvance(updateSamba, data, "Setup - Samba", "Samba configuration saved successfully"),
    [handleSubmitAndAdvance, updateSamba]
  );

  const handleAuthorized = useCallback(() => {
    checkAll();
  }, [checkAll]);

  const handleBootScriptSubmit = useCallback(async (content) => {
    info(`Updating Boot Script`);
    try {
      if (!await handleConfigSave("tftp-autoexec", content)) return;
      await checkAll();
      success("Setup - Boot Script", "Boot Script saved successfully");
    } catch (e) {
      error("Setup Wizard", `Failed to update boot script: ${e}`);
    }
  }, [info, handleConfigSave, success, error, checkAll]);

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
          hasBootScript
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
      hasBootScript,
    ]
  );

  const finishSetup = async () => {
    setCompleting(true);
    setSetupError("");
    try {
      const status = await completeSetup();
      if (!status.completed || !status.ready) throw new Error("Server setup is not ready");
      return true;
    } catch (e) {
      await checkAll();
      setSetupError(e.message || "Unable to complete setup");
      return false;
    } finally { setCompleting(false); }
  };

  return {
    setupStatus, setupError, completing, finishSetup,
    activeStep,
    setActiveStep,
    checking,
    installing,
    disks,
    poolExists,
    poolName,
    dependencies,
    steps,
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
    privilegedAccessGranted,
    authChecking: checking,
  };
};
