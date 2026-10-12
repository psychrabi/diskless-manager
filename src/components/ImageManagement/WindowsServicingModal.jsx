import { getDriverReadiness, listMasters } from "@/api/modules/images";
import { listBootTests, listDriverRevisions, listNetworkDrivers } from "@/api/modules/networkDrivers";
import {
  getRemoteWindowsServicingCapabilities,
  prepareWindowsImageManaged,
  prepareWindowsImageRemote,
  prepareWindowsImageRemoteCatalog,
} from "@/api/modules/windowsServicing";
import { Button, Modal, Select } from "@/components/ui";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import {
  CheckCircle2,
  FlaskConical,
  Network,
  PackageCheck,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

const DEFAULTS = {
  host: "",
  username: "Administrator",
  password: "",
  executablePath: "",
  imageId: "",
  snapshot: "",
  newName: "",
  portal: "",
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

function formatBytes(value) {
  if (!Number.isFinite(value)) return "";
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KiB";
  if (value < 1024 * 1024 * 1024) {
    return (value / (1024 * 1024)).toFixed(1) + " MiB";
  }
  return (value / (1024 * 1024 * 1024)).toFixed(2) + " GiB";
}

export default function WindowsServicingModal({ isOpen, onClose, onPrepared }) {
  const [form, setForm] = useState(DEFAULTS);
  const [worker, setWorker] = useState(null);
  const [result, setResult] = useState(null);
  const [checking, setChecking] = useState(false);
  const [preparing, setPreparing] = useState(false);
  const [imageMode, setImageMode] = useState("managed");
  const [images, setImages] = useState([]);
  const [driverSource, setDriverSource] = useState("catalog");
  const [drivers, setDrivers] = useState([]);
  const [driversLoading, setDriversLoading] = useState(true);
  const [selectedDriverIds, setSelectedDriverIds] = useState([]);
  const [readiness, setReadiness] = useState(null);
  const [revisions, setRevisions] = useState([]);
  const [testStates, setTestStates] = useState({});
  const { success, error } = useToastStore();
  const confirm = useConfirm();

  useEffect(() => {
    if (!isOpen) return undefined;

    let active = true;
    listMasters().then((items) => {
      if (active) setImages(items.filter((image) => image.os_type === "windows" && image.status === "ready"));
    }).catch((err) => {
      if (active) error("Windows Servicing", "Failed to load images: " + (err?.message || String(err)));
    });
    listNetworkDrivers()
      .then((items) => {
        if (active) setDrivers(Array.isArray(items) ? items : []);
      })
      .catch((err) => {
        if (active) {
          setDrivers([]);
          error(
            "Windows Servicing",
            "Failed to load imported network drivers: " +
              (err?.message || String(err))
          );
        }
      })
      .finally(() => {
        if (active) setDriversLoading(false);
      });

    return () => {
      active = false;
    };
  }, [isOpen, error]);

  useEffect(() => {
    if (!isOpen || imageMode !== "managed" || !form.imageId) {
      return undefined;
    }
    let active = true;
    getDriverReadiness(form.imageId)
      .then((value) => {
        if (active) setReadiness(value);
      })
      .catch(() => {
        if (active) setReadiness(null);
      });
    listDriverRevisions(form.imageId)
      .then(async (items) => {
        if (!active) return;
        const list = Array.isArray(items) ? items : [];
        setRevisions(list);
        const states = {};
        for (const revision of list.filter((item) => item.status === "awaiting-test")) {
          try {
            const tests = await listBootTests(revision.id);
            const latest = Array.isArray(tests) ? tests[tests.length - 1] : null;
            states[revision.id] = latest?.status ?? "untested";
          } catch {
            states[revision.id] = "unknown";
          }
        }
        if (active) setTestStates(states);
      })
      .catch(() => {
        if (active) setRevisions([]);
      });
    return () => {
      active = false;
    };
  }, [isOpen, imageMode, form.imageId]);

  const canRun = useMemo(() => {
    const driverReady =
      driverSource === "catalog"
        ? selectedDriverIds.length > 0
        : Boolean(form.driverRoot.trim());

    return Boolean(
      worker?.available &&
        worker?.elevated &&
        form.host.trim() &&
        form.username.trim() &&
        (imageMode === "managed"
          ? worker.iscsi_volume_servicing && form.imageId && form.snapshot && form.newName.trim() && form.portal.trim()
          : form.imagePath.trim() && Number(form.imageIndex) > 0) &&
        driverReady
    );
  }, [worker, form, driverSource, selectedDriverIds, imageMode]);

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

  const bootArm = () => ({
    allow_dirty_hive: form.allowDirtyHive,
    increase_disk_timeout: form.increaseDiskTimeout,
    disable_task_offload: form.disableTaskOffload,
    disable_crash_dump: form.disableCrashDump,
    clear_paging_files: form.clearPagingFiles,
  });

  const manualPreparation = (commit) => ({
    image_path: form.imagePath.trim(),
    driver_root: form.driverRoot.trim(),
    image_index: Number(form.imageIndex),
    recursive: true,
    commit,
    boot_arm: bootArm(),
  });

  const toggleDriver = (id) => {
    setSelectedDriverIds((current) =>
      current.includes(id)
        ? current.filter((existing) => existing !== id)
        : [...current, id]
    );
  };

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
      if (!capabilities.available) {
        error(
          "Windows Servicing",
          "Worker responded, but DISM/reg.exe servicing dependencies are unavailable."
        );
      } else if (!capabilities.elevated) {
        error(
          "Windows Servicing",
          "Worker tools are available, but the SSH session is not elevated. Use an administrator account/token."
        );
      } else {
        success(
          "Windows Servicing",
          "Worker ready (" +
            capabilities.platform +
            ", protocol " +
            capabilities.protocol_version +
            ", elevated)."
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
    if (!canRun || preparing) return;

    if (commit) {
      const ok = await confirm({
        title: "Commit Windows Image Preparation",
        description:
          imageMode === "managed"
            ? "This will clone the selected snapshot, prepare its NIC drivers, and save a new image and snapshot after disconnecting the worker. The source image stays intact. Continue?"
            : "This will install the selected driver package(s), arm the offline SYSTEM hive for network boot, and commit the WIM. Test with Dry Run first. Continue?",
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
      let response;
      if (imageMode === "managed") {
        response = await prepareWindowsImageManaged({
          ...remoteIdentity(), image_id: form.imageId, source_snapshot: form.snapshot,
          new_name: form.newName.trim(), portal: form.portal.trim(), commit, boot_arm: bootArm(),
          ...(driverSource === "catalog" ? { driver_ids: selectedDriverIds } : { driver_root: form.driverRoot.trim() }),
        });
        if (commit) onPrepared?.();
      } else if (driverSource === "catalog") {
        response = await prepareWindowsImageRemoteCatalog({
          ...remoteIdentity(),
          image_path: form.imagePath.trim(),
          image_index: Number(form.imageIndex),
          commit,
          boot_arm: bootArm(),
          driver_ids: selectedDriverIds,
        });
      } else {
        response = await prepareWindowsImageRemote({
          ...remoteIdentity(),
          preparation: manualPreparation(commit),
        });
      }

      const preparation = response.preparation || response;
      setResult({
        preparation,
        image: response.image,
        snapshot: response.snapshot,
        staging: response.preparation && response.staged_package_count > 0
          ? {
              packageCount: response.staged_package_count,
              uploadedBytes: response.uploaded_bytes,
              cleanupSucceeded: response.staging_cleanup_succeeded,
            }
          : null,
      });

      success(
        "Windows Servicing",
        commit
          ? "Windows image preparation committed successfully."
          : imageMode === "managed"
            ? "Dry run completed; the temporary clone was deleted."
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
    setImageMode("managed");
    setDriverSource("catalog");
    setSelectedDriverIds([]);
    onClose?.();
  };

  const preparationResult = result?.preparation;
  const plan = preparationResult?.boot_arm?.plan;

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
                The Linux server can stage imported driver packages over verified
                SSH/SFTP, run DISM and SYSTEM-hive arming on the Windows worker,
                then remove the temporary staging directory.
              </p>
            </div>
          </div>
        </div>

        <fieldset disabled={preparing} className="space-y-6 border-0 p-0">
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
                placeholder="Leave blank for SSH agent or server ~/.ssh key"
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
                {worker.available && worker.elevated
                  ? "Ready · " +
                    worker.platform +
                    " · protocol " +
                    worker.protocol_version +
                    " · elevated"
                  : !worker.available
                    ? "Worker dependencies unavailable"
                    : "SSH session is not elevated"}
              </span>
            )}
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-semibold">Windows image</h3>
          <Select label="Image source" value={imageMode} disabled={preparing} onChange={(event) => setImageMode(event.target.value)}>
            <option value="managed">Managed image snapshot</option>
            <option value="wim">WIM file on worker</option>
          </Select>
          {imageMode === "managed" ? (
            <div className="grid gap-3 md:grid-cols-2">
              <Select label="Source Windows image" value={form.imageId} disabled={preparing} onChange={(event) => {
                update("imageId", event.target.value); update("snapshot", "");
                setReadiness(null); setRevisions([]); setTestStates({});
              }}>
                <option value="">Select an image</option>
                {images.map((image) => <option key={image.id} value={image.id}>{image.name}</option>)}
              </Select>
              <Select label="Source snapshot" value={form.snapshot} disabled={preparing} onChange={(event) => update("snapshot", event.target.value)}>
                <option value="">Select a snapshot</option>
                {images.find((image) => image.id === form.imageId)?.snapshots?.map((snapshot) => (
                  <option key={snapshot.name} value={snapshot.name}>{snapshot.name}</option>
                ))}
              </Select>
              <div className="space-y-1.5">
                <Label htmlFor="serviced-image-name">New image name</Label>
                <Input id="serviced-image-name" value={form.newName} onChange={(event) => update("newName", event.target.value)} placeholder="windows11-serviced" />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="servicing-portal">Server iSCSI address</Label>
                <Input id="servicing-portal" value={form.portal} onChange={(event) => update("portal", event.target.value)} placeholder="Server address reachable from worker" />
              </div>
              <p className="text-xs text-muted-foreground md:col-span-2">
                Choose a snapshot that already boots from iSCSI. Preparation uses a separate clone. Dry Run deletes that clone; committing saves a new image and snapshot.
              </p>
              {form.imageId && (
                <div className="rounded-md border border-border/60 p-3 text-xs md:col-span-2" data-testid="driver-readiness">
                  {readiness ? (
                    <p className="text-muted-foreground">
                      Latest tested revision: {readiness.latest_ready_revision ? readiness.latest_ready_revision.id.slice(0, 8) : "none"}
                      {" · "}awaiting test: {readiness.candidate_count}
                      {" · "}failed: {readiness.failed_count}
                    </p>
                  ) : (
                    <p className="text-muted-foreground">Driver revision status unavailable.</p>
                  )}
                  {revisions.filter((item) => item.status === "awaiting-test").map((item) => (
                    <p key={item.id} className="mt-1 text-muted-foreground">
                      Candidate {item.id.slice(0, 8)}: boot test {testStates[item.id] ?? "…"}.
                      New clients stay on the latest tested revision until it passes.
                    </p>
                  ))}
                </div>
              )}
              {worker && !worker.iscsi_volume_servicing && <p role="alert" className="text-sm text-destructive md:col-span-2">Update the Windows helper to enable managed image servicing.</p>}
            </div>
          ) : (
          <div className="grid gap-3 md:grid-cols-2">
            <div className="space-y-1.5 md:col-span-2">
              <Label htmlFor="windows-image-path">WIM image path on worker</Label>
              <Input
                id="windows-image-path"
                value={form.imagePath}
                onChange={(event) => update("imagePath", event.target.value)}
                placeholder="D:\Images\install.wim"
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
          )}
        </section>

        <section className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h3 className="text-sm font-semibold">Network drivers</h3>
              <p className="text-xs text-muted-foreground">
                Use imported packages for managed staging, or point at a folder
                that already exists on the Windows worker.
              </p>
            </div>
            <div className="flex rounded-md border p-1">
              <button
                type="button"
                onClick={() => setDriverSource("catalog")}
                className={
                  "rounded px-3 py-1.5 text-xs font-medium " +
                  (driverSource === "catalog"
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground")
                }
              >
                Imported packages
              </button>
              <button
                type="button"
                onClick={() => setDriverSource("remote")}
                className={
                  "rounded px-3 py-1.5 text-xs font-medium " +
                  (driverSource === "remote"
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground")
                }
              >
                Existing worker folder
              </button>
            </div>
          </div>

          {driverSource === "catalog" ? (
            <div className="space-y-2">
              <div className="flex items-center justify-between gap-3 text-xs">
                <span className="text-muted-foreground">
                  {driversLoading
                    ? "Loading imported drivers..."
                    : selectedDriverIds.length +
                      " selected of " +
                      drivers.length}
                </span>
                {drivers.length > 0 && (
                  <div className="flex gap-2">
                    <button
                      type="button"
                      className="font-medium text-primary"
                      onClick={() =>
                        setSelectedDriverIds(drivers.map((driver) => driver.id))
                      }
                    >
                      Select all
                    </button>
                    <button
                      type="button"
                      className="font-medium text-muted-foreground"
                      onClick={() => setSelectedDriverIds([])}
                    >
                      Clear
                    </button>
                  </div>
                )}
              </div>

              <div className="max-h-64 space-y-2 overflow-y-auto rounded-md border p-2">
                {!driversLoading && drivers.length === 0 && (
                  <div className="p-4 text-center text-sm text-muted-foreground">
                    No imported network driver packages are available.
                  </div>
                )}
                {drivers.map((driver) => {
                  const checked = selectedDriverIds.includes(driver.id);
                  return (
                    <label
                      key={driver.id}
                      className={
                        "flex cursor-pointer items-start gap-3 rounded-md border p-3 " +
                        (checked
                          ? "border-primary/50 bg-primary/5"
                          : "border-border/60")
                      }
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleDriver(driver.id)}
                        className="mt-1 size-4"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-2 font-medium">
                          <PackageCheck className="size-4 shrink-0" />
                          <span className="truncate">{driver.name}</span>
                        </span>
                        <span className="mt-1 block text-xs text-muted-foreground">
                          {[driver.provider, driver.version]
                            .filter(Boolean)
                            .join(" · ") || "Imported network driver"}
                        </span>
                        {driver.architectures?.length > 0 && (
                          <span className="mt-1 block text-xs text-muted-foreground">
                            Architectures: {driver.architectures.join(", ")}
                          </span>
                        )}
                        {driver.hardware_ids?.length > 0 && (
                          <span className="mt-1 block break-all font-mono text-[11px] text-muted-foreground">
                            {driver.hardware_ids.slice(0, 2).join(" · ")}
                            {driver.hardware_ids.length > 2
                              ? " · +" + (driver.hardware_ids.length - 2)
                              : ""}
                          </span>
                        )}
                      </span>
                    </label>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="space-y-1.5">
              <Label htmlFor="windows-driver-root">
                Existing driver package root on worker
              </Label>
              <Input
                id="windows-driver-root"
                value={form.driverRoot}
                onChange={(event) => update("driverRoot", event.target.value)}
                placeholder="D:\DisklessDrivers"
              />
            </div>
          )}
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

        </fieldset>
        <div className="flex flex-wrap items-center gap-2 border-t pt-4">
          <Button
            variant="outline"
            icon={FlaskConical}
            loading={preparing}
            disabled={!canRun || preparing}
            onClick={() => runPreparation(false)}
          >
            Dry Run
          </Button>
          <Button
            variant="primary"
            icon={ShieldCheck}
            loading={preparing}
            disabled={!canRun || preparing}
            onClick={() => runPreparation(true)}
          >
            Prepare and Commit
          </Button>
          <Button variant="ghost" disabled={preparing} onClick={resetAndClose}>
            Close
          </Button>
        </div>

        {preparationResult && (
          <section className="space-y-4 rounded-md border border-border/60 p-4">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="font-semibold">
                  {preparationResult.committed
                    ? "Committed result"
                    : "Dry-run result"}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {preparationResult.drivers_added} INF package(s) processed ·
                  index {preparationResult.image_index}
                </p>
                {result.image && (
                  <p className="mt-1 text-sm">Saved {result.image.name}@{result.snapshot?.name}. Select this image and snapshot for the client’s boot test.</p>
                )}
                {result.staging && (
                  <p className="mt-1 text-xs text-muted-foreground">
                    Staged {result.staging.packageCount} imported package(s) ·{" "}
                    {formatBytes(result.staging.uploadedBytes)} uploaded ·
                    cleanup{" "}
                    {result.staging.cleanupSucceeded ? "completed" : "needs attention"}
                  </p>
                )}
              </div>
              <span
                className={
                  preparationResult.committed
                    ? "rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-700"
                    : "rounded-full bg-muted px-2.5 py-1 text-xs font-medium"
                }
              >
                {preparationResult.committed ? "Committed" : "Discarded"}
              </span>
            </div>

            {result.staging && !result.staging.cleanupSucceeded && (
              <div className="rounded-md border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
                <div className="flex items-center gap-2 font-medium">
                  <ShieldAlert className="size-4" />
                  Driver staging cleanup did not complete
                </div>
                <p className="mt-1 text-muted-foreground">
                  The image operation succeeded, but the temporary staging
                  directory on the Windows worker should be inspected and removed.
                </p>
              </div>
            )}

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
                  <div>
                    <span className="text-muted-foreground">
                      Selected NIC driver services
                    </span>
                    <p className="font-mono">
                      {plan.selected_nic_services?.join(", ") || "None selected"}
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
