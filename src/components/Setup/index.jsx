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
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import BootScriptStep from "./BootScriptStep";
import DependencyStep from "./DependencyStep";
import DHCPStep from "./DHCPStep";
import FinishedStep from "./FinishedStep";
import HTTPStep from "./HTTPStep";
import SambaStep from "./SambaStep";
import StorageStep from "./StorageStep";
import TFTPStep from "./TFTPStep";

const Setup = () => {
  const navigate = useNavigate();
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [installing, setInstalling] = useState("");
  const [activeStep, setActiveStep] = useState(1);
  const [checking, setChecking] = useState(false);
  const [bootScriptContent, setBootScriptContent] = useState(null);
  const { appConfig, fetchConfig } = useAppStore();
  const { error, success, info } = useToastStore();
  const { updateDhcp, updateTftp, updateHttp } = useSettings();
  const { handleConfigSave, fetchServiceConfig } = useServiceManager();
  
  // Extract pool name from config
  const poolName = appConfig?.settings?.zpool_name || appConfig?.settings?.zfsPool || "zroot";

  const { dependencies, fetchDependencies } = useAppStore(
    useShallow((state) => ({
      dependencies: state.dependencies || [],
      fetchDependencies: state.fetchDependencies,
    }))
  );

  const checkAll = useCallback(async () => {
    setChecking(true);
    try {
      const d = await listDisks();
      setDisks(d);

      const exists = await checkZfsPoolExists();
      setPoolExists(exists);

      await fetchDependencies();
      await fetchConfig();
      
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
  }, [fetchDependencies, fetchConfig]);

  useEffect(() => {
    checkAll();
  }, [checkAll]);

  const allServicesInstalled =
    dependencies.length > 0 && !dependencies.some((svc) => !svc.installed);

  // Determine the first incomplete step
  useEffect(() => {
    if (allServicesInstalled && poolExists && appConfig?.settings?.dhcp && appConfig?.settings?.tftp && appConfig?.settings?.http && appConfig?.settings?.samba && bootScriptContent) {
      // All steps complete, show finished
      setActiveStep(8);
    } else if (allServicesInstalled && poolExists && appConfig?.settings?.dhcp && appConfig?.settings?.tftp && appConfig?.settings?.http && appConfig?.settings?.samba) {
      // Boot script not saved
      setActiveStep(7);
    } else if (allServicesInstalled && poolExists && appConfig?.settings?.dhcp && appConfig?.settings?.tftp && appConfig?.settings?.http) {
      // Samba not configured
      setActiveStep(6);
    } else if (allServicesInstalled && poolExists && appConfig?.settings?.dhcp && appConfig?.settings?.tftp) {
      // HTTP not configured
      setActiveStep(5);
    } else if (allServicesInstalled && poolExists && appConfig?.settings?.dhcp) {
      // TFTP not configured
      setActiveStep(4);
    } else if (allServicesInstalled && poolExists) {
      // DHCP not configured
      setActiveStep(3);
    } else if (allServicesInstalled) {
      // Storage not configured
      setActiveStep(2);
    } else {
      // Dependencies not installed
      setActiveStep(1);
    }
  }, [allServicesInstalled, poolExists, appConfig?.settings?.dhcp, appConfig?.settings?.tftp, appConfig?.settings?.http, appConfig?.settings?.samba, bootScriptContent]);

  // Auto-progress logic - only advance if we are on the current step
  const prevInstalled = useRef(allServicesInstalled);
  const prevPool = useRef(poolExists);

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

  const handleDhcpSubmit = async (data) => {
    const ok = await updateDhcp(data);
    if (ok) setActiveStep(4);
    success("Setup - DHCP", "DHCP configuration saved successfully");
  };

  const handleTftpSubmit = async (data) => {
    const ok = await updateTftp(data);
    if (ok) setActiveStep(5);
    success("Setup - TFTP", "TFTP configuration saved successfully");
  };

  const handleHttpSubmit = async (data) => {
    const ok = await updateHttp(data);
    if (ok) setActiveStep(6);
    success("Setup - HTTP", "HTTP configuration saved successfully");
  };

  const handleSambaSubmit = async (shares) => {
    try {
      await configureSambaServer(shares);
      success("Setup - Samba", "Samba configuration saved successfully");
      setActiveStep(7);
    } catch (e) {
      error("Setup Wizard", `Failed to configure Samba: ${e}`);
    }
  };

  const handleBootScriptSubmit = async (content) => {
    info(`Updating Boot Script`);
    try {
      await handleConfigSave("tftp-autoexec", content);
      setBootScriptContent(content);
      setActiveStep(8);
      success("Setup - Boot Script", "Boot Script saved successfully");
    } catch (e) {
      error("Setup Wizard", `Failed to update boot script: ${e}`);
    }
  };

  const steps = [
    {
      id: 1,
      title: "Dependencies",
      icon: Package,
      status: allServicesInstalled ? "complete" : "current",
    },
    {
      id: 2,
      title: "Storage",
      icon: Database,
      status: poolExists
        ? "complete"
        : allServicesInstalled
        ? "current"
        : "upcoming",
    },
    {
      id: 3,
      title: "DHCP",
      icon: Network,
      status: appConfig?.settings?.dhcp
        ? "complete"
        : activeStep === 3
        ? "current"
        : "upcoming",
    },
    {
      id: 4,
      title: "TFTP",
      icon: Network,
      status: appConfig?.settings?.tftp
        ? "complete"
        : activeStep === 4
        ? "current"
        : "upcoming",
    },
    {
      id: 5,
      title: "HTTP",
      icon: Globe,
      status: appConfig?.settings?.http
        ? "complete"
        : activeStep === 5
        ? "current"
        : "upcoming",
    },
    {
      id: 6,
      title: "Samba",
      icon: Share2,
      status: appConfig?.settings?.samba
        ? "complete"
        : activeStep === 6
        ? "current"
        : "upcoming",
    },
    {
      id: 7,
      title: "Boot",
      icon: Code,
      status:
        activeStep > 7 ? "complete" : activeStep === 7 ? "current" : "upcoming",
    },
    {
      id: 8,
      title: "Finished",
      icon: CheckCircle,
      status: activeStep === 8 ? "current" : "upcoming",
    },
  ];

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
          <DependencyStep
            dependencies={dependencies}
            checking={checking}
            onRefresh={checkAll}
            onInstall={handleInstallService}
            installing={installing}
          />
        )}

        {activeStep === 2 && (
          <StorageStep
            disks={disks}
            poolExists={poolExists}
            poolName={poolName}
            onSubmit={handleCreatePool}
          />
        )}

        {activeStep === 3 && (
          <DHCPStep
            onSubmit={handleDhcpSubmit}
            initialConfig={appConfig?.settings?.dhcp}
          />
        )}

        {activeStep === 4 && (
          <TFTPStep
            onSubmit={handleTftpSubmit}
            initialConfig={appConfig?.settings?.tftp}
          />
        )}

        {activeStep === 5 && (
          <HTTPStep
            onSubmit={handleHttpSubmit}
            initialConfig={appConfig?.settings?.http}
          />
        )}

        {activeStep === 6 && (
          <SambaStep
            onSubmit={handleSambaSubmit}
            initialConfig={appConfig?.settings?.samba?.[0]}
          />
        )}

        {activeStep === 7 && (
          <BootScriptStep onSubmit={handleBootScriptSubmit} />
        )}

        {activeStep === 8 && (
          <FinishedStep
            onNavigateHome={() => {
              setActiveStep(8);
              navigate("/");
            }}
          />
        )}
      </div>

      {activeStep < 8 ? (
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
