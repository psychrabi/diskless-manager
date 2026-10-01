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
import BackupManagement from "@/components/ApplicationManagement/BackupManagement";
import NetworkConfig from "@/components/SettingsManagement/NetworkConfig";

const Setup = () => {
  const navigate = useNavigate();
  const wizard = useSetupWizard();
  const {
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
    authChecking,
  } = wizard;

  return (
    <div className="w-full max-w-4xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500 px-2 sm:px-0">
      <div className="text-center space-y-2">
        <h1 className="text-3xl sm:text-4xl font-semibold tracking-tight text-foreground">
          System Setup
        </h1>
        <p className="text-muted-foreground text-lg">
          Configure your server for diskless booting
        </p>
      </div>

      <details>
        <summary className="cursor-pointer">Restore an existing application backup</summary>
        <div className="mt-4"><BackupManagement /></div>
      </details>
      {setupError && <div role="alert"><p>{setupError}</p><Button onClick={checkAll}>Retry</Button></div>}
      <nav aria-label="Setup steps" className="flex justify-between items-center gap-2 p-4 relative overflow-x-auto">
        {steps.map((step) => (
          <Button
            key={step.id}
            variant="ghost"
            aria-current={activeStep === step.id ? "step" : undefined}
            className="relative z-10 h-auto flex-col gap-2 group shrink-0 px-3 py-2"
            disabled={checking || !setupStatus || (step.id === 9 ? !setupStatus.ready : steps.slice(0, step.id - 1).some(previous => previous.status !== "complete"))}
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
          <div className="flex flex-col gap-4">
          <NetworkConfig applyOnSave={false} onSaved={checkAll} />
          <p className="text-sm text-muted-foreground">Use the same server address, subnet mask, and gateway in the DHCP configuration below.</p>
          <DHCPStep
            onSubmit={handleDhcpSubmit}
            initialConfig={appConfig?.settings?.dhcp}
          />
          </div>
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
            initialConfig={appConfig?.settings?.samba}
          />
        )}

        {activeStep === 8 && (
          <div className="flex flex-col gap-4">
            <BootScriptStep onSubmit={handleBootScriptSubmit} />
            {setupStatus?.missing_files?.length > 0 && <div role="alert"><p>Missing or empty boot files:</p><ul className="list-disc pl-6">{setupStatus.missing_files.map(path => <li key={path}><code>{path}</code></li>)}</ul></div>}
            <p className="text-sm text-muted-foreground">Place the configured PXE bootloader binaries ({[appConfig?.settings?.dhcp?.boot_file_legacy, appConfig?.settings?.dhcp?.boot_file_uefi32, appConfig?.settings?.dhcp?.boot_file_uefi64].filter(Boolean).join(", ") || "undionly.kpxe, ipxe.efi, snponly.efi"}) in {appConfig?.settings?.tftp?.root_dir || "/srv/tftp"}. Saving the boot script does not install these binaries.</p>
            <Button onClick={checkAll} disabled={checking}>Refresh server readiness</Button>
          </div>
        )}

        {activeStep === 9 && setupStatus?.ready && (
          <FinishedStep
            completing={completing}
            onNavigateHome={async () => {
              if (await finishSetup()) navigate("/", { replace: true });
            }}
          />
        )}
      </div>

      <p className="text-xs text-muted-foreground">{checking ? "Checking server readiness…" : setupStatus?.ready ? "Review your existing configuration and confirm setup to open the dashboard." : "Complete the required server configuration to continue."}</p>
    </div>
  );
};

export default Setup;
