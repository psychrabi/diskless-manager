import {
  ChevronRight,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useSetupWizard } from "@/hooks/useSetupWizard";
import AuthorizeStep from "./AuthorizeStep";
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
  const wizard = useSetupWizard();
  const {
    activeStep, setActiveStep, checking, installing, disks, poolExists,
    poolName, dependencies, steps,
    appConfig, checkAll, handleCreatePool, handleInstallService,
    handleDhcpSubmit, handleTftpSubmit, handleHttpSubmit, handleSambaSubmit,
    handleAuthorized, handleBootScriptSubmit,
  } = wizard;

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
            Status: {checking ? "Refreshing\u2026" : "Configuration in progress"}
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
