import {
  getRemoteWindowsServicingCapabilities,
  prepareWindowsImageRemote,
} from "@/api/modules/windowsServicing";
import { Button, Modal } from "@/components/ui";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import {
  CheckCircle2,
  FlaskConical,
  Network,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { useMemo, useState } from "react";

const DEFAULTS = {
  host: "",
  username: "Administrator",
  password: "",
  executablePath: "",
  imagePath: "",
  driverRoot: "",
  imageIndex: 1,
  allowDirtyHive: false,
  increaseDiskTimeout: true,
  disableTaskOffload: false,
  disableCrashDump: false,
  clearPagingFiles: false,
};

function Toggle({ id, checked, onChange, label, description, danger = false }) {
  return (
    <label
      htmlFor={id}
      className="flex items-start gap-3 rounded-md border border-border/60 p-3"
    >
      <input
        id={id}
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-1 size-4"
      />
      <span className="space-y-1">
        <span className={danger ? "font-medium text-destructive" : "font-medium"}>
          {label}
        </span>
        {description && (
          <span className="block text-xs text-muted-foreground">{description}</span>
        )}
      </span>
    </label>
  );
}

function formatRegistryValue(value) {
  if (!value || typeof value !== "object") return String(value ?? "");
  if (value.type === "dword") return "DWORD " + value.value;
  if (value.type === "multi_string") {
    return value.value?.length ? value.value.join(", ") : "(empty multi-string)";
  }
  return JSON.stringify(value);
}

export default function WindowsServicingModal({ isOpen, onClose }) {
  const [form, setForm] = useState(DEFAULTS);
  const [worker, setWorker] = useState(null);
  const [result, setResult] = useState(null);
  const [checking, setChecking] = useState(false);
  const [preparing, setPreparing] = useState(false);
  const { success, error } = useToastStore();
  const confirm = useConfirm();

  const canRun = useMemo(
    () =>
      Boolean(
        worker?.available &&
          form.host.trim() &&
          form.username.trim() &&
          form.imagePath.trim() &&
          form.driverRoot.trim() &&
          Number(form.imageIndex) > 0
      ),
    [worker, form]
  );

  const update = (field, value) => {
    setForm((current) => ({ ...current, [field]: value }));
    if (["host", "username", "password", "executablePath"].includes(field)) {
      setWorker(null);
    }
  };

  const remoteIdentity = () => ({
    host: form.host.trim(),
    username: form.username.trim(),
    ...(form.password ? { password: form.password } : {}),
    ...(form.executablePath.trim()
      ? { executable_path: form.executablePath.trim() }
      : {}),
  });

  const preparation = (commit) => ({
    image_path: form.imagePath.trim(),
    driver_root: form.driverRoot.trim(),
    image_index: Number(form.imageIndex),
    recursive: true,
    commit,
    boot_arm: {
      allow_dirty_hive: form.allowDirtyHive,
      increase_disk_timeout: form.increaseDiskTimeout,
      disable_task_offload: form.disableTaskOffload,
      disable_crash_dump: form.disableCrashDump,
      clear_paging_files: form.clearPagingFiles,
    },
  });

  const checkWorker = async () => {
    if (!form.host.trim() || !form.username.trim()) {
      error("Windows Servicing", "Host and username are required.");
      return;
    }

    setChecking(true);
    setResult(null);
    try {
      const capabilities = await getRemoteWindowsServicingCapabilities(
        remoteIdentity()
      );
      setWorker(capabilities);
      if (capabilities.available) {
        success(
          "Windows Servicing",
          "Worker ready (" +
            capabilities.platform +
            ", protocol " +
            capabilities.protocol_version +
            ")."
        );
      } else {
        error(
          "Windows Servicing",
          "Worker responded, but DISM/reg.exe servicing dependencies are unavailable."
        );
      }
    } catch (err) {
      setWorker(null);
      error("Windows Servicing", err?.message || String(err));
    } finally {
      setChecking(false);
    }
  };

  const runPreparation = async (commit) => {
    if (!canRun) return;

    if (commit) {
      const ok = await confirm({
        title: "Commit Windows Image Preparation",
        description:
          "This will install the selected driver package(s), arm the offline SYSTEM hive for network boot, and commit the WIM. Test with Dry Run first. Continue?",
        confirmText: "Prepare and Commit",
        cancelText: "Cancel",
        confirmVariant: "destructive",
        size: "2xl",
      });
      if (!ok) return;
    }

    setPreparing(true);
    setResult(null);
    try {
      const response = await prepareWindowsImageRemote({
        ...remoteIdentity(),
        preparation: preparation(commit),
      });
      setResult(response);
      success(
        "Windows Servicing",
        commit
          ? "Windows image preparation committed successfully."
          : "Dry run completed; all DISM changes were discarded."
      );
    } catch (err) {
      error("Windows Servicing", err?.message || String(err));
    } finally {
      setPreparing(false);
    }
  };

  const resetAndClose = () => {
    if (preparing || checking) return;
    setForm(DEFAULTS);
    setWorker(null);
    setResult(null);
    onClose?.();
  };

  const plan = result?.boot_arm?.plan;

  return (
    <Modal
      isOpen={isOpen}
      onClose={resetAndClose}
      title="Prepare Windows Network-Boot Image"
      size="4xl"
      className="max-h-[90vh] overflow-y-auto"
    >
      <div className="space-y-6">
        <div className="rounded-md border border-border/60 bg-muted/30 p-4 text-sm">
          <div className="flex items-start gap-2">
            <Network className="mt-0.5 size-4 shrink-0" />
            <div>
              <p className="font-medium">Remote Windows servicing worker</p>
              <p className="mt-1 text-muted-foreground">
                Paths below are on the Windows servicing machine, not on the
                Linux diskless server. SSH host-key verification remains enabled.
              </p>
            </div>
          </div>
        </div>

        <section className="space-y-3">
          <h3 className="text-sm font-semibold">Worker connection</h3>
          <div className="grid gap-3 md:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="servicing-host">Windows host</Label>
              <Input
                id="servicing-host"
                value={form.host}
                onChange={(event) => update("host", event.target.value)}
                placeholder="192.168.1.50 or win-servicer"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="servicing-user">SSH username</Label>
              <Input
                id="servicing-user"
                value={form.username}
                onChange={(event) => update("username", event.target.value)}
                placeholder="Administrator"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="servicing-password">SSH password</Label>
              <Input
                id="servicing-password"
                type="password"
                value={form.password}
                onChange={(event) => update("password", event.target.value)}
                placeholder="Leave blank for SSH agent authentication"
                autoComplete="new-password"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="servicing-helper">Helper executable</Label>
              <Input
                id="servicing-helper"
                value={form.executablePath}
                onChange={(event) =>
                  update("executablePath", event.target.value)
                }
                placeholder="Default: C:\Program Files\Diskless Manager\diskless-windows-servicer.exe"
              />
            </div>
          </div>

          <div className="flex items-center gap-3">
            <Button
              variant="outline"
              icon={CheckCircle2}
              loading={checking}
              onClick={checkWorker}
            >
              Check Worker
            </Button>
            {worker && (
              <span
                className={
                  worker.available
                    ? "text-sm text-emerald-600"
                    : "text-sm text-destructive"
                }
              >
                {worker.available
                  ? "Ready · " +
                    worker.platform +
                    " · protocol " +
                    worker.protocol_version
                  : "Worker dependencies unavailable"}
              </span>
            )}
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-semibold">Windows image</h3>
          <div className="grid gap-3 md:grid-cols-2">
            <div className="space-y-1.5 md:col-span-2">
              <Label htmlFor="windows-image-path">WIM image path</Label>
              <Input
                id="windows-image-path"
                value={form.imagePath}
                onChange={(event) => update("imagePath", event.target.value)}
                placeholder="D:\Images\install.wim"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="windows-driver-root">Driver package root</Label>
              <Input
                id="windows-driver-root"
                value={form.driverRoot}
                onChange={(event) => update("driverRoot", event.target.value)}
                placeholder="D:\DisklessDrivers"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="windows-image-index">WIM index</Label>
              <Input
                id="windows-image-index"
                type="number"
                min="1"
                value={form.imageIndex}
                onChange={(event) => update("imageIndex", event.target.value)}
              />
            </div>
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-semibold">Boot compatibility policy</h3>
          <div className="grid gap-2 md:grid-cols-2">
            <Toggle
              id="disk-timeout"
              checked={form.increaseDiskTimeout}
              onChange={(value) => update("increaseDiskTimeout", value)}
              label="Increase disk timeout"
              description="Sets the existing disk service TimeOutValue to 60 seconds."
            />
            <Toggle
              id="task-offload"
              checked={form.disableTaskOffload}
              onChange={(value) => update("disableTaskOffload", value)}
              label="Disable TCP task offload"
              description="Compatibility option; disabled by default."
            />
            <Toggle
              id="crash-dump"
              checked={form.disableCrashDump}
              onChange={(value) => update("disableCrashDump", value)}
              label="Disable crash dumps"
              description="Avoids dump-stack use of the boot NIC."
            />
            <Toggle
              id="paging"
              checked={form.clearPagingFiles}
              onChange={(value) => update("clearPagingFiles", value)}
              label="Clear paging files"
              description="Keeps paging traffic off the network boot volume."
            />
            <Toggle
              id="dirty-hive"
              checked={form.allowDirtyHive}
              onChange={(value) => update("allowDirtyHive", value)}
              label="Allow dirty SYSTEM hive"
              description="Recovery/testing only. Normal servicing should leave this disabled."
              danger
            />
          </div>
        </section>

        <div className="flex flex-wrap items-center gap-2 border-t pt-4">
          <Button
            variant="outline"
            icon={FlaskConical}
            loading={preparing}
            disabled={!canRun}
            onClick={() => runPreparation(false)}
          >
            Dry Run
          </Button>
          <Button
            variant="primary"
            icon={ShieldCheck}
            loading={preparing}
            disabled={!canRun}
            onClick={() => runPreparation(true)}
          >
            Prepare and Commit
          </Button>
          <Button variant="ghost" disabled={preparing} onClick={resetAndClose}>
            Close
          </Button>
        </div>

        {result && (
          <section className="space-y-4 rounded-md border border-border/60 p-4">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="font-semibold">
                  {result.committed ? "Committed result" : "Dry-run result"}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {result.drivers_added} INF package(s) processed · index{" "}
                  {result.image_index}
                </p>
              </div>
              <span
                className={
                  result.committed
                    ? "rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-700"
                    : "rounded-full bg-muted px-2.5 py-1 text-xs font-medium"
                }
              >
                {result.committed ? "Committed" : "Discarded"}
              </span>
            </div>

            {plan && (
              <>
                <div className="grid gap-3 text-sm md:grid-cols-2">
                  <div>
                    <span className="text-muted-foreground">
                      Active control set
                    </span>
                    <p className="font-mono">{plan.active_control_set}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">
                      Native NIC services
                    </span>
                    <p className="font-mono">
                      {plan.native_nic_services?.join(", ") || "None detected"}
                    </p>
                  </div>
                </div>

                {plan.warnings?.length > 0 && (
                  <div className="rounded-md border border-amber-500/30 bg-amber-500/5 p-3">
                    <div className="mb-2 flex items-center gap-2 font-medium">
                      <ShieldAlert className="size-4" />
                      Warnings
                    </div>
                    <ul className="space-y-1 text-sm text-muted-foreground">
                      {plan.warnings.map((warning) => (
                        <li key={warning}>• {warning}</li>
                      ))}
                    </ul>
                  </div>
                )}

                <div>
                  <h4 className="mb-2 text-sm font-semibold">
                    Registry changes ({plan.changes?.length || 0})
                  </h4>
                  <div className="max-h-72 overflow-auto rounded-md border">
                    <table className="w-full text-left text-xs">
                      <thead className="sticky top-0 bg-muted">
                        <tr>
                          <th className="p-2 font-medium">Key</th>
                          <th className="p-2 font-medium">Value</th>
                          <th className="p-2 font-medium">New data</th>
                          <th className="p-2 font-medium">Reason</th>
                        </tr>
                      </thead>
                      <tbody>
                        {plan.changes?.map((change, index) => (
                          <tr
                            key={
                              change.key +
                              "-" +
                              change.name +
                              "-" +
                              String(index)
                            }
                            className="border-t align-top"
                          >
                            <td className="max-w-72 break-all p-2 font-mono">
                              {change.key}
                            </td>
                            <td className="p-2 font-mono">{change.name}</td>
                            <td className="p-2 font-mono">
                              {formatRegistryValue(change.value)}
                            </td>
                            <td className="max-w-72 p-2 text-muted-foreground">
                              {change.reason}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </>
            )}
          </section>
        )}
      </div>
    </Modal>
  );
}
