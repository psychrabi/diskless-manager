import { ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
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
    authChecking,
  } = wizard;

  return (
    <div className="w-full max-w-3xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500 px-2 sm:px-0">
      <div className="text-center space-y-2">
        <h1 className="text-3xl sm:text-4xl font-semibold tracking-tight text-foreground">
          System Setup
        </h1>
        <p className="text-muted-foreground text-lg">
          Configure your server for diskless booting
        </p>
      </div>

      <nav aria-label="Setup steps" className="flex justify-between items-center gap-2 p-4 relative overflow-x-auto">
        {steps.map((step) => (
          <Button
            key={step.id}
            variant="ghost"
            aria-current={activeStep === step.id ? "step" : undefined}
            className="relative z-10 h-auto flex-col gap-2 group shrink-0 px-3 py-2"
            onClick={() => setActiveStep(step.id)}
          >
            <div
              className={`w-12 h-12 rounded-full flex items-center justify-center border-4 transition-all duration-300 ${
                step.status === "complete"
                  ? "bg-emerald-600 border-emerald-600 text-white scale-110 group-hover:bg-emerald-600/80"
                  : step.status === "current" || activeStep === step.id
                    ? "bg-primary border-primary text-primary-foreground scale-110 shadow-lg shadow-primary/20"
                    : "bg-background border-border text-muted-foreground group-hover:border-primary/50"
              }`}
            >
              <step.icon size={20} />
            </div>
            <span
              className={`mt-2 text-xs sm:text-sm font-bold whitespace-nowrap ${
                activeStep === step.id
                  ? "text-primary"
                  : step.status === "upcoming"
                    ? "text-muted-foreground"
                    : "text-foreground"
              }`}
            >
              {step.title}
            </span>
          </Button>
        ))}
      </nav>

      <div className="min-h-[50vh]">
        {activeStep === 1 && (
          <AuthorizeStep
            onAuthorized={handleAuthorized}
            authorized={privilegedAccessGranted}
            checking={authChecking}
          />
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
        <div className="flex justify-between items-center text-xs text-muted-foreground">
          <span>
            Status:{" "}
            {checking ? "Refreshing\u2026" : "Configuration in progress"}
          </span>
          <Button
            variant="link"
            size="sm"
            onClick={() => navigate("/")}
          >
            Skip for now <ChevronRight size={14} />
          </Button>
        </div>
      ) : (
        <div className="flex justify-between items-center text-xs text-muted-foreground">
          <span>Status: Setup completed</span>
          <Button
            variant="link"
            size="sm"
            onClick={() => navigate("/")}
          >
            Go to Dashboard <ChevronRight size={14} />
          </Button>
        </div>
      )}
    </div>
  );
};

export default Setup;
