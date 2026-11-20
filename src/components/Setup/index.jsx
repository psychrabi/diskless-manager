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
    <div className="ui-enter mx-auto w-full max-w-4xl space-y-8 px-2 sm:px-0">
      <div className="space-y-2 text-center sm:text-left">
        <h1 className="text-balance text-3xl font-semibold tracking-[-0.035em] text-foreground sm:text-4xl">
          System setup
        </h1>
        <p className="text-pretty text-base leading-7 text-muted-foreground sm:text-lg">
          Configure this server for diskless booting, one step at a time.
        </p>
      </div>

      <details className="rounded-xl border border-border/70 bg-card/40 px-4 py-3">
        <summary className="cursor-pointer text-sm font-medium text-foreground">Restore an existing application backup</summary>
        <div className="mt-4"><BackupManagement /></div>
      </details>
      {setupError && (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          <p>{setupError}</p>
          <Button variant="outline" onClick={checkAll}>Retry check</Button>
        </div>
      )}
      <details className="rounded-xl border border-border bg-card p-4 lg:hidden">
        <summary className="cursor-pointer text-sm font-semibold">Step {activeStep} of {steps.length} — {steps.find(step => step.id === activeStep)?.title}</summary>
        <ol className="mt-3 grid grid-cols-2 gap-2">
          {steps.map((step) => (
            <li key={step.id}>
              <Button variant={activeStep === step.id ? "secondary" : "ghost"} className="h-auto sm:h-auto min-h-11 w-full justify-start whitespace-normal text-left" aria-current={activeStep === step.id ? "step" : undefined}
                disabled={checking || !setupStatus || (step.id === 9 ? !setupStatus.ready : steps.slice(0, step.id - 1).some(previous => previous.status !== "complete"))}
                onClick={() => setActiveStep(step.id)}>
                {step.id}. {step.title}{step.status === "complete" ? " · Complete" : ""}
              </Button>
            </li>
          ))}
        </ol>
      </details>
      <nav aria-label="Setup steps" className="relative hidden lg:flex items-center justify-between gap-2 overflow-x-auto rounded-2xl border border-border/70 bg-card/40 p-4">
        {steps.map((step) => (
          <Button
            key={step.id}
            variant="ghost"
            aria-current={activeStep === step.id ? "step" : undefined}
            className="group relative z-10 h-auto sm:h-auto shrink-0 flex-col gap-2 px-3 py-2"
            disabled={checking || !setupStatus || (step.id === 9 ? !setupStatus.ready : steps.slice(0, step.id - 1).some(previous => previous.status !== "complete"))}
            onClick={() => setActiveStep(step.id)}
          >
            <div
              className={`flex h-12 w-12 items-center justify-center rounded-full border-4 transition-all duration-200 ease-out ${
                step.status === "complete"
                  ? "scale-105 border-success bg-success text-white"
                  : step.status === "current" || activeStep === step.id
                    ? "scale-105 border-primary bg-primary text-primary-foreground shadow-lg shadow-primary/20"
                    : "border-border bg-background text-muted-foreground group-hover:border-primary/50"
              }`}
            >
              <step.icon size={20} aria-hidden="true" />
            </div>
            <span
              className={`mt-1 whitespace-nowrap text-xs font-semibold sm:text-sm ${
                activeStep === step.id
                  ? "text-primary"
                  : step.status === "upcoming"
                    ? "text-muted-foreground"
                    : "text-foreground"
              }`}
            >
              {step.title}
              {step.status === "complete" && <span className="block text-[10px] font-normal">Complete</span>}
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

      <p className="flex items-center gap-2 text-xs text-muted-foreground sm:text-sm" aria-live="polite">
        {checking ? "Checking server readiness…" : setupStatus?.ready ? "Configuration is ready. Review and confirm to open the dashboard." : "Complete the required steps to finish server setup."}
      </p>
    </div>
  );
};

export default Setup;
