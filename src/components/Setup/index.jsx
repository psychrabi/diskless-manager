import { useServiceManager } from "@/hooks/useServiceManager";
import { useSettings } from "@/hooks/useSettings";
import { useToastStore } from "@/store/useToastStore";
import {
  listDisks,
  checkZfsPoolExists,
  createZfsPool,
  installService,
  configureSambaServer,
} from "@/api/commands";
import {
  CheckCircle,
  ChevronRight,
  Code,
  Database,
  Globe,
  Network,
  Package,
  Share2,
  Shield,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import AuthorizeStep from "./AuthorizeStep";
import BootScriptStep from "./BootScriptStep";
import DependencyStep from "./DependencyStep";
import DHCPStep from "./DHCPStep";
import FinishedStep from "./FinishedStep";
import HTTPStep from "./HTTPStep";
import SambaStep from "./SambaStep";
import StorageStep from "./StorageStep";
import TFTPStep from "./TFTPStep";

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

const Setup = () => {
  const navigate = useNavigate();
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

  // Extract pool name from config
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

      // Check if boot script exists - call directly without adding to dependencies
      try {
        const bootConfig = await fetchServiceConfig("tftp-autoexec");
        if (bootConfig?.text) {
          setBootScriptContent(bootConfig.text);
        }
      } catch (e) {
        // Boot script fetch failed, that's ok
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

  // Determine the first incomplete step
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

  // Auto-progress logic - only advance if we are on the current step
  // const prevInstalled = useRef(allServicesInstalled);
  // const prevPool = useRef(poolExists);

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

  const handleSambaSubmit = async (shares) => {
    try {
      await configureSambaServer(shares);
      success("Setup - Samba", "Samba configuration saved successfully");
      setActiveStep(8);
    } catch (e) {
      error("Setup Wizard", `Failed to configure Samba: ${e}`);
    }
  };

  const handleAuthorized = useCallback(() => {
    setPrivilegedAccessGranted(true);
    setActiveStep(2);
  }, []);

  const handleBootScriptSubmit = async (content) => {
    info(`Updating Boot Script`);
    try {
      await handleConfigSave("tftp-autoexec", content);
      setBootScriptContent(content);
      setActiveStep(9);
      success("Setup - Boot Script", "Boot Script saved successfully");
    } catch (e) {
      error("Setup Wizard", `Failed to update boot script: ${e}`);
    }
  };

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

  return (
    <div className="max-w-3xl min-w-3xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="text-center space-y-2">
        <h1 className="text-4xl font-black tracking-tight bg-gradient-to-r from-primary to-secondary bg-clip-text text-transparent">
          System Setup
        </h1>
        <p className="text-base-content/60 text-lg">
          Configure your server for diskless booting
        </p>
      </div>

      {/* Modern Stepper */}
      <div className="flex justify-between items-center px-4 relative">
        {steps.map((step) => (
          <div
            key={step.id}
            className="relative z-10 flex flex-col items-center cursor-pointer group"
            onClick={() => setActiveStep(step.id)}
          >
            <div
              className={`w-12 h-12 rounded-full flex items-center justify-center border-4 transition-all duration-300 ${
                step.status === "complete"
                  ? "bg-success border-success text-success-content scale-110 group-hover:bg-success/80"
                  : step.status === "current" || activeStep === step.id
                  ? "bg-primary border-primary text-primary-content scale-110 shadow-lg shadow-primary/20"
                  : "bg-base-100 border-base-300 text-base-content/40 group-hover:border-primary/50"
              }`}
            >
              <step.icon size={20} />
            </div>
            <span
              className={`mt-2 text-sm font-bold ${
                activeStep === step.id
                  ? "text-primary"
                  : step.status === "upcoming"
                  ? "text-base-content/40"
                  : "text-base-content"
              }`}
            >
              {step.title}
            </span>
          </div>
        ))}
      </div>

      <div className="min-h-[calc(100vh-32rem)]">
        {activeStep === 1 && (
          <AuthorizeStep onAuthorized={handleAuthorized} />
        )}

        {activeStep === 2 && (
          <DependencyStep
            dependencies={dependencies}
            checking={checking}
            onRefresh={checkAll}
            onInstall={handleInstallService}
            installing={installing}
          />
        )}

        {activeStep === 3 && (
          <StorageStep
            disks={disks}
            poolExists={poolExists}
            poolName={poolName}
            onSubmit={handleCreatePool}
          />
        )}

        {activeStep === 4 && (
          <DHCPStep
            onSubmit={handleDhcpSubmit}
            initialConfig={appConfig?.settings?.dhcp}
          />
        )}

        {activeStep === 5 && (
          <TFTPStep
            onSubmit={handleTftpSubmit}
            initialConfig={appConfig?.settings?.tftp}
          />
        )}

        {activeStep === 6 && (
          <HTTPStep
            onSubmit={handleHttpSubmit}
            initialConfig={appConfig?.settings?.http}
          />
        )}

        {activeStep === 7 && (
          <SambaStep
            onSubmit={handleSambaSubmit}
            initialConfig={appConfig?.settings?.samba?.[0]}
          />
        )}

        {activeStep === 8 && (
          <BootScriptStep onSubmit={handleBootScriptSubmit} />
        )}

        {activeStep === 9 && (
          <FinishedStep
            onNavigateHome={() => {
              setActiveStep(9);
              navigate("/");
            }}
          />
        )}
      </div>

      {activeStep < 9 ? (
        <div className="flex justify-between items-center text-xs text-base-content/40">
          <span>
            Status: {checking ? "Refreshing..." : "Configuration in progress"}
          </span>
          <span
            className="flex items-center gap-1 cursor-pointer hover:text-primary transition-colors"
            onClick={() => navigate("/")}
          >
            Skip for now <ChevronRight size={14} />
          </span>
        </div>
      ) : (
        <div className="flex justify-between items-center text-xs text-base-content/40">
          <span>Status: Setup completed</span>
          <span
            className="flex items-center gap-1 cursor-pointer hover:text-primary transition-colors"
            onClick={() => navigate("/")}
          >
            Go to Dashboard <ChevronRight size={14} />
          </span>
        </div>
      )}
    </div>
  );
};

export default Setup;
