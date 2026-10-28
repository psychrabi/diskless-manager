import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import {
  addClient,
  deleteClientWriteback,
  getClientNvmeOfStatus,
  prepareClientNvmeOf,
  removeClientNvmeOf,
  rotateChapSecret,
  updateClient,
} from "@/api/modules/clients";
import { prepareWindowsAssignment } from "@/api/modules/windowsServicing";
import { listGameDisks } from "@/api/modules/zfs";
import { readConfig } from "@/api/modules/config";
import ClientGameDiskPicker from "./ClientGameDiskPicker";
import { clientSchema } from "@/schema";
import { useAuth } from "@/contexts/auth";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Copy, Eye, EyeOff, KeyRound, RefreshCw, Save, Server, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useForm, useWatch } from "react-hook-form";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import { Progress } from "@/components/ui/progress";
import { useConfirm } from "@/contexts/confirmDialog";

// Each opened client owns its form and asynchronous status state. Responses
// belonging to an unmounted client cannot overwrite another client's dialog.
const ClientFormModal = (props) => (
  <ClientFormModalContent key={`${props.client?.id ?? "new"}:${props.isOpen}`} {...props} />
);

// Explicit form defaults: a new client starts enabled with CHAP off, the
// game switch off and an empty selection, instead of `undefined` values.
const formDefaults = {
  enabled: true,
  keep_writeback: false,
  chap_enabled: false,
  use_game_disk: false,
  game_disks: [],
};

const ClientFormModalContent = ({ client, masters, isOpen, onClose, refresh }) => {
  const { success, error } = useToastStore();
  const { user } = useAuth();
  const chapFeatureEnabled = useAppStore(
    (state) => state.appConfig?.settings?.client_features?.enable_chap_authentication === true,
  );
  const nvmeFeatureEnabled = useAppStore(
    (state) => state.appConfig?.settings?.client_features?.enable_experimental_nvme_target === true,
  );
  // The persisted store copy goes stale as soon as the worker is configured
  // elsewhere, and a stale profile silently skips driver preparation. Read the
  // live settings whenever the dialog opens instead of trusting the cache.
  const storedWindowsServicingProfile = useAppStore(
    (state) => state.appConfig?.settings?.windows_servicing,
  );
  const [liveWindowsServicingProfile, setLiveWindowsServicingProfile] = useState(null);
  const windowsServicingProfile =
    liveWindowsServicingProfile ?? storedWindowsServicingProfile;
  const windowsServicingConfigured = Boolean(
    windowsServicingProfile?.worker_host?.trim() &&
    windowsServicingProfile?.worker_username?.trim() &&
    windowsServicingProfile?.iscsi_portal?.trim() &&
    windowsServicingProfile?.report_server_url?.trim(),
  );
  const confirm = useConfirm();
  const [nvmeStatus, setNvmeStatus] = useState(null);
  const [nvmeLoading, setNvmeLoading] = useState(Boolean(isOpen && client?.id && nvmeFeatureEnabled));
  const [nvmeAction, setNvmeAction] = useState(null);
  const [nvmeError, setNvmeError] = useState("");
  // `null` means not fetched yet (loading); an array may be empty.
  const [gameDisks, setGameDisks] = useState(null);
  const [deletingWriteback, setDeletingWriteback] = useState(false);
  const [preparingWindowsDrivers, setPreparingWindowsDrivers] = useState(false);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting, isDirty, isValid, dirtyFields },
    setValue,
    control,
    reset,
  } = useForm({
    mode: "onChange",
    resolver: zodResolver(clientSchema),
    defaultValues: { ...formDefaults, ...client },
  });

  useEffect(() => {
    if (isOpen && client) {
      reset({
        ...formDefaults,
        ...client,
      });
    }
  }, [client, isOpen, reset]);

  // Game masters power the per-client picker. A failed fetch degrades to
  // the zero-state hint instead of breaking the form. State updates only
  // happen in the async continuations, never synchronously in the effect.
  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    listGameDisks().then(
      (data) => {
        if (cancelled) return;
        setGameDisks(data?.disks ?? []);
      },
      () => {
        if (cancelled) return;
        setGameDisks([]);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    readConfig().then(
      (config) => {
        if (!cancelled) setLiveWindowsServicingProfile(config?.settings?.windows_servicing ?? {});
      },
      () => {},
    );
    return () => { cancelled = true; };
  }, [isOpen]);

  const loadNvmeStatus = useCallback(async () => {
    if (!isOpen || !client?.id || !nvmeFeatureEnabled) {
      setNvmeStatus(null);
      setNvmeError("");
      return;
    }

    setNvmeLoading(true);
    setNvmeError("");
    try {
      const status = await getClientNvmeOfStatus(client.id);
      setNvmeStatus(status);
    } catch (e) {
      setNvmeStatus(null);
      setNvmeError(e?.message || String(e));
    } finally {
      setNvmeLoading(false);
    }
  }, [client, isOpen, nvmeFeatureEnabled]);

  useEffect(() => {
    if (!isOpen || !client?.id || !nvmeFeatureEnabled) return;
    let cancelled = false;
    getClientNvmeOfStatus(client.id).then(
      (status) => {
        if (cancelled) return;
        setNvmeStatus(status);
        setNvmeError("");
        setNvmeLoading(false);
      },
      (failure) => {
        if (cancelled) return;
        setNvmeStatus(null);
        setNvmeError(failure?.message || String(failure));
        setNvmeLoading(false);
      },
    );
    return () => { cancelled = true; };
  }, [client?.id, isOpen, nvmeFeatureEnabled]);

  const resolveNvmeServerIp = async () => {
    const config = await readConfig();
    return (
      config?.settings?.dhcp?.next_server_ip ||
      config?.settings?.server?.ip_address ||
      config?.settings?.http?.server_ip ||
      ""
    ).trim();
  };

  const handlePrepareNvme = async () => {
    if (!client?.id || !nvmeFeatureEnabled) return;

    setNvmeAction("prepare");
    setNvmeError("");
    try {
      const serverIp = await resolveNvmeServerIp();
      if (!serverIp) {
        throw new Error("No diskless server IP is configured.");
      }

      const preparation = await prepareClientNvmeOf(client.id, serverIp);
      setNvmeStatus(preparation.export);
      success(
        "NVMe/TCP",
        `NVMe/TCP target prepared for ${client.name} on ${serverIp}:4420.`
      );
    } catch (e) {
      const message = e?.message || String(e);
      setNvmeError(message);
      error("NVMe/TCP", e);
    } finally {
      setNvmeAction(null);
    }
  };

  const handleRemoveNvme = async () => {
    if (!client?.id) return;

    setNvmeAction("remove");
    setNvmeError("");
    try {
      await removeClientNvmeOf(client.id);
      success("NVMe/TCP", `NVMe/TCP target removed for ${client.name}.`);
      await loadNvmeStatus();
    } catch (e) {
      const message = e?.message || String(e);
      setNvmeError(message);
      error("NVMe/TCP", e);
    } finally {
      setNvmeAction(null);
    }
  };

  const onSubmit = async (data) => {
    let request = {
      name: data.name,
      mac: data.mac,
      ip: data.ip,
      master: data.master,
      snapshot: data.snapshot || null,
      keep_writeback: data.keep_writeback,
      use_game_disk: data.use_game_disk,
      game_disks: data.game_disks || [],
      enabled: data.enabled ?? true,
      ...(chapFeatureEnabled ? { chap_enabled: Boolean(data.chap_enabled) } : {}),
    };
    try {
      let driverMessage = "";
      // Driver preparation can hand back a different image than the one the form
      // shows. It must be part of the update even when the image fields were not
      // edited, or the client keeps booting an image without its Boot NIC driver.
      let preparedAssignment = false;
      const selectedImage = masters?.find((image) => image.name === data.master);
      // A rename, IP or writeback change cannot alter which Boot NIC drivers
      // the image needs, so only a new client or a changed image/snapshot/MAC
      // asks the worker to prepare one. Preparing on every save made an
      // offline servicing worker block edits that had nothing to do with it.
      const assignmentChanged =
        !client?.id ||
        Boolean(dirtyFields.master) ||
        Boolean(dirtyFields.snapshot) ||
        Boolean(dirtyFields.mac);
      if (
        assignmentChanged &&
        windowsServicingConfigured &&
        selectedImage?.os_type === "windows"
      ) {
        setPreparingWindowsDrivers(true);
        try {
          const prepared = await prepareWindowsAssignment({
            image_name: data.master,
            snapshot: request.snapshot,
            mac: data.mac,
          });
          if (prepared.prepared) {
            request = { ...request, master: prepared.master, snapshot: prepared.snapshot };
            preparedAssignment = true;
            driverMessage = prepared.reused
              ? ` Reused prepared Windows image '${prepared.master}' with matching NIC drivers.`
              : ` Windows NIC drivers were prepared in '${prepared.master}'.`;
          } else if (prepared.already_covered) {
            driverMessage = ` '${prepared.master}' already includes this client’s Boot NIC drivers.`;
          } else {
            throw new Error("The selected Windows image was not prepared with this client’s catalog drivers.");
          }
        } catch (prepareError) {
          const proceed = await confirm({
            title: "Windows driver preparation failed",
            description: `${prepareError?.message || String(prepareError)} You can assign the original image anyway, but Windows may not boot if its Boot NIC driver is missing.`,
            confirmText: "Assign original image",
            cancelText: "Cancel",
            size: "2xl",
          });
          if (!proceed) return;
          driverMessage = " Windows driver preparation failed; the original image was assigned.";
        } finally {
          setPreparingWindowsDrivers(false);
        }
      }
      if (!client?.id) {
        await addClient(request);
        success("Client Management", `Client ${data.name} added successfully.${driverMessage}`);
      } else {
        const editableFields = [
          "name",
          "mac",
          "ip",
          "keep_writeback",
          "use_game_disk",
          "game_disks",
          "chap_enabled",
          "enabled",
        ];
        const update = Object.fromEntries(
          editableFields
            .filter((field) => dirtyFields[field])
            .map((field) => [field, request[field]]),
        );
        if (
          preparedAssignment ||
          dirtyFields.master ||
          dirtyFields.snapshot
        ) {
          update.master = request.master;
          update.snapshot = request.snapshot;
        }
        await updateClient(client.id, update);
        success(
          "Client Management",
          `Client ${data.name} updated successfully.${driverMessage}`
        );
      }
      onClose();
      reset();
      await refresh();
    } catch (e) {
      error("Client Management", e);
    }
  };

  const selectedMaster = useWatch({
    control,
    name: "master",
  });

  const selectedMasterOs = masters?.find((master) => master.name === selectedMaster)?.os_type;

  const keepWriteback = useWatch({
    control,
    name: "keep_writeback",
  });

  const clientEnabled = useWatch({
    control,
    name: "enabled",
  });

  const chapEnabled = useWatch({
    control,
    name: "chap_enabled",
  });

  const [chapSecret, setChapSecret] = useState(null);
  const [chapUsername, setChapUsername] = useState(null);
  const [chapRevealed, setChapRevealed] = useState(false);
  const [chapRotating, setChapRotating] = useState(false);

  const handleRotateChap = async () => {
    if (!client?.id || chapRotating) return;
    setChapRotating(true);
    try {
      const result = await rotateChapSecret(client.id);
      setChapSecret(result?.password ?? null);
      setChapUsername(result?.username ?? null);
      setChapRevealed(true);
      success("Client Management", result?.message || "CHAP secret rotated.");
    } catch (e) {
      error("Client Management", e);
    } finally {
      setChapRotating(false);
    }
  };

  const handleCopyChapSecret = async () => {
    if (!chapSecret) return;
    try {
      await navigator.clipboard.writeText(chapSecret);
      success("Client Management", "CHAP secret copied to clipboard.");
    } catch {
      error("Client Management", "Could not access the clipboard.");
    }
  };

  const handleDeleteWriteback = async () => {
    const ok = await confirm({
      title: "Delete Writeback",
      description: "All changes in this client's boot and game clones will be lost. Storage will be recreated from its configured sources on the next PXE boot.",
      confirmText: "Delete Writeback",
      cancelText: "Cancel",
      confirmVariant: "destructive",
      size: "2xl",
    });
    if (!ok || deletingWriteback) return;
    setDeletingWriteback(true);
    try {
      const result = await deleteClientWriteback(client.id);
      success("Client Management", result?.message || "Writeback storage deleted.");
      await refresh?.();
    } catch (e) {
      error("Client Management", e);
    } finally {
      setDeletingWriteback(false);
    }
  };

  const useGameDisk = useWatch({
    control,
    name: "use_game_disk",
  });

  const selectedGameDisks = useWatch({
    control,
    name: "game_disks",
  });

  const toggleGameDisk = (dataset) => {
    const current = selectedGameDisks ?? [];
    const next = current.includes(dataset)
      ? current.filter((disk) => disk !== dataset)
      : [...current, dataset];
    setValue("game_disks", next, {
      shouldValidate: true,
      shouldDirty: true,
    });
  };

  const handleGameDiskSwitch = (checked) => {
    const enabled = Boolean(checked);
    setValue("use_game_disk", enabled, {
      shouldValidate: true,
      shouldDirty: true,
    });
    if (enabled) {
      // Pre-check every discovered master when enabling with an empty
      // selection; the user can deselect from there.
      const discovered = gameDisks ?? [];
      if ((selectedGameDisks ?? []).length === 0 && discovered.length > 0) {
        setValue(
          "game_disks",
          discovered.map((disk) => disk.dataset),
          { shouldValidate: true, shouldDirty: true },
        );
      }
    } else {
      setValue("game_disks", [], {
        shouldValidate: true,
        shouldDirty: true,
      });
    }
  };

  const nvmeReady = Boolean(
    nvmeStatus?.subsystem_present &&
      nvmeStatus?.namespace_enabled &&
      nvmeStatus?.port_attached
  );

  const masterReg = register("master");
  const { onChange: masterRegOnChange, ...masterRegRest } = masterReg;

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open && !deletingWriteback) onClose();
      }}
    >
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{client?.id ? "Edit Client" : "Add Client"}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4">
          {preparingWindowsDrivers && (
            <Alert aria-live="polite">
              <AlertDescription className="space-y-2">
                Preparing a Windows image with catalog NIC drivers. This can take several minutes.
                <Progress value={null} aria-label="Preparing Windows NIC drivers" />
              </AlertDescription>
            </Alert>
          )}
          <Field>
            <FieldLabel htmlFor="name">Client Name</FieldLabel>
            <FieldContent>
              <Input
                id="name"
                type="text"
                placeholder="enter client name"
                aria-invalid={!!errors.name}
                {...register("name")}
              />
              <FieldError>{errors.name?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field>
            <FieldLabel htmlFor="mac">MAC Address</FieldLabel>
            <FieldContent>
              <Input
                id="mac"
                type="text"
                placeholder="XX:XX:XX:XX:XX:XX"
                aria-invalid={!!errors.mac}
                {...register("mac")}
              />
              <FieldError>{errors.mac?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field>
            <FieldLabel htmlFor="ip">IP Address</FieldLabel>
            <FieldContent>
              <Input
                id="ip"
                type="text"
                placeholder="X.X.X.X"
                aria-invalid={!!errors.ip}
                {...register("ip")}
              />
              <FieldError>{errors.ip?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field orientation="horizontal">
            <Checkbox
              id="client-enabled"
              checked={clientEnabled ?? true}
              onCheckedChange={(checked) =>
                setValue("enabled", Boolean(checked), {
                  shouldValidate: true,
                  shouldDirty: true,
                })
              }
            />
            <FieldContent>
              <FieldLabel htmlFor="client-enabled" className="cursor-pointer">
                Enabled
              </FieldLabel>
              <FieldDescription>
                If unchecked, this machine is denied at boot even when it is fully provisioned
              </FieldDescription>
            </FieldContent>
          </Field>

          <Field>
            <FieldLabel htmlFor="master">Select Image</FieldLabel>
            <FieldContent>
              <NativeSelect
                id="master"
                aria-invalid={!!errors.master}
                {...masterRegRest}
                onChange={(e) => {
                  masterRegOnChange(e);
                  setValue("snapshot", "", {
                    shouldValidate: true,
                    shouldDirty: true,
                  });
                }}
              >
                <NativeSelectOption value="">Select image ...</NativeSelectOption>
                {masters?.map((master) => (
                  <NativeSelectOption key={master.name} value={master.name}>
                    {master.name}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <FieldError>{errors.master?.message}</FieldError>
              {selectedMasterOs === "windows" && !client?.id && (
                <p className="text-xs text-muted-foreground" data-testid="windows-driver-hint">
                  Boot tests do not block assignment. {windowsServicingConfigured
                    ? "Saving prepares the image with catalog drivers for inventoried clients."
                    : "Configure the Windows servicing worker in System Settings to prepare catalog NIC drivers automatically."}
                </p>
              )}
            </FieldContent>
          </Field>

          <Field>
            <FieldLabel htmlFor="snapshot">Select Snapshot</FieldLabel>
            <FieldContent>
              <NativeSelect
                id="snapshot"
                aria-invalid={!!errors.snapshot}
                disabled={!selectedMaster}
                {...register("snapshot")}
              >
                <NativeSelectOption value="">Use master directly</NativeSelectOption>
                {masters?.find((m) => m.name === selectedMaster)
                  ?.snapshots?.map((snap) => {
                    const label = snap.name.includes("@")
                      ? snap.name.split("@").pop()
                      : snap.name;
                    const snapshotSource = snap.name.includes("@")
                      ? snap.name
                      : `${selectedMaster}@${snap.name}`;
                    return (
                      <NativeSelectOption key={snap.name} value={snapshotSource}>
                        {label} ({snap.created}, {snap.size})
                      </NativeSelectOption>
                    );
                  })}
              </NativeSelect>
              <FieldError>{errors.snapshot?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field orientation="horizontal">
            <Checkbox
              id="keep-writeback"
              checked={Boolean(keepWriteback)}
              onCheckedChange={(checked) =>
                setValue("keep_writeback", Boolean(checked), {
                  shouldValidate: true,
                  shouldDirty: true,
                })
              }
            />
            <FieldContent>
              <FieldLabel htmlFor="keep-writeback" className="cursor-pointer">
                Keep Writeback (Persistent Mode)
              </FieldLabel>
              <FieldDescription>
                If unchecked, the clone resets after the offline delay configured in Settings
                (non-persistent mode)
              </FieldDescription>
            </FieldContent>
          </Field>

          <div>
            <Field orientation="horizontal">
              <Checkbox
                id="use-game-disk"
                checked={Boolean(useGameDisk)}
                onCheckedChange={handleGameDiskSwitch}
              />
              <FieldContent>
                <FieldLabel htmlFor="use-game-disk" className="cursor-pointer">
                  Use Game Disk
                </FieldLabel>
                <FieldDescription>
                  If checked, each selected game disk gets a private writable
                  clone attached to this client via iSCSI
                </FieldDescription>
              </FieldContent>
            </Field>
            {Boolean(useGameDisk) && (
              <div className="mt-3">
                <ClientGameDiskPicker
                  masters={gameDisks ?? []}
                  loading={gameDisks === null}
                  selected={selectedGameDisks ?? []}
                  onToggle={toggleGameDisk}
                  onNavigateAway={onClose}
                />
              </div>
            )}
          </div>

          {chapFeatureEnabled && (
            <div>
              <Field orientation="horizontal">
                <Checkbox
                  id="chap-enabled"
                  checked={chapEnabled ?? false}
                  onCheckedChange={(checked) =>
                    setValue("chap_enabled", Boolean(checked), {
                      shouldValidate: true,
                      shouldDirty: true,
                    })
                  }
                />
                <FieldContent>
                  <FieldLabel htmlFor="chap-enabled" className="cursor-pointer">
                    CHAP Authentication
                  </FieldLabel>
                  <FieldDescription>
                    If checked, the iSCSI target requires the login secret
                    below. The boot menu carries it automatically.
                  </FieldDescription>
                </FieldContent>
              </Field>
              {client?.id && (
                <div className="mt-3 flex flex-col gap-2 rounded-lg border border-border bg-muted/30 p-3">
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <KeyRound className="size-4 text-muted-foreground" />
                    iSCSI login secret
                  </div>
                  <div className="grid gap-1 text-sm">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-muted-foreground">Username</span>
                      <span className="">{chapUsername || "—"}</span>
                    </div>
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-muted-foreground">Secret</span>
                      <span className="flex items-center gap-1">
                        <span className="">
                          {chapSecret ? (chapRevealed ? chapSecret : "••••••••••••") : "—"}
                        </span>
                        {chapSecret && (
                          <>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              title={chapRevealed ? "Hide secret" : "Show secret"}
                              onClick={() => setChapRevealed((value) => !value)}
                            >
                              {chapRevealed ? <EyeOff /> : <Eye />}
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              title="Copy secret"
                              onClick={handleCopyChapSecret}
                            >
                              <Copy />
                            </Button>
                          </>
                        )}
                      </span>
                    </div>
                  </div>
                  <div>
                    <Button
                      type="button"
                      variant="ghost"
                      icon={RefreshCw}
                      disabled={chapRotating || user?.role !== "admin"}
                      onClick={handleRotateChap}
                    >
                      {chapRotating ? "Rotating..." : "Rotate secret"}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Login credentials are shown only after an administrator rotates the secret.
                    Rotation applies to the live target at once; existing
                    sessions stay up, new logins need the new secret, and the
                    boot menu is republished automatically.
                  </p>
                </div>
              )}
            </div>
          )}

          {client?.id && nvmeFeatureEnabled && (
            <div className="rounded-lg border border-border bg-muted/30 p-4">
              <div className="mb-3 flex items-start justify-between gap-3">
                <div>
                  <div className="flex items-center gap-2 font-semibold">
                    <Server className="size-4" />
                    Experimental NVMe/TCP
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Exposes this client's existing ZVOL through Linux NVMe/TCP.
                    iSCSI remains the normal/default boot path.
                  </p>
                </div>
                <Badge
                  variant={
                    nvmeLoading
                      ? "secondary"
                      : nvmeReady
                        ? "default"
                        : "outline"
                  }
                >
                  {nvmeLoading ? "Checking..." : nvmeReady ? "Ready" : "Not ready"}
                </Badge>
              </div>

              <div className="grid gap-2 text-sm sm:grid-cols-2">
                <div>
                  <span className="text-muted-foreground">NQN:</span>
                  <div className="break-all  text-xs">
                    {nvmeStatus?.nqn || "—"}
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">Block device:</span>
                  <div className="break-all  text-xs">
                    {nvmeStatus?.block_device || client?.block_device || client?.block_store || "—"}
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">Namespace:</span>{" "}
                  <span>{nvmeStatus?.namespace_enabled ? "Enabled" : "Disabled"}</span>
                </div>
                <div>
                  <span className="text-muted-foreground">TCP port:</span>{" "}
                  <span>{nvmeStatus?.tcp_port || 4420}</span>
                </div>
                <div>
                  <span className="text-muted-foreground">Subsystem:</span>{" "}
                  <span>{nvmeStatus?.subsystem_present ? "Present" : "Missing"}</span>
                </div>
                <div>
                  <span className="text-muted-foreground">Port attached:</span>{" "}
                  <span>{nvmeStatus?.port_attached ? "Yes" : "No"}</span>
                </div>
              </div>

              {nvmeError && (
                <Alert variant="destructive" className="mt-3">
                  <AlertDescription>{nvmeError}</AlertDescription>
                </Alert>
              )}

              <div className="mt-4 flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="default"
                  disabled={Boolean(nvmeAction)}
                  onClick={handlePrepareNvme}
                >
                  {nvmeAction === "prepare" ? (
                    <Spinner data-icon="inline-start" />
                  ) : (
                    <Server data-icon="inline-start" />
                  )}
                  {nvmeAction === "prepare"
                    ? "Preparing..."
                    : nvmeReady
                      ? "Refresh NVMe/TCP Target"
                      : "Prepare NVMe/TCP Target"}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={Boolean(nvmeAction) || nvmeLoading}
                  onClick={loadNvmeStatus}
                >
                  <RefreshCw data-icon="inline-start" />
                  Refresh Status
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={Boolean(nvmeAction) || !nvmeStatus?.subsystem_present}
                  onClick={handleRemoveNvme}
                >
                  <Trash2 data-icon="inline-start" />
                  {nvmeAction === "remove" ? "Removing..." : "Remove NVMe Target"}
                </Button>
              </div>
            </div>
          )}

          <DialogFooter className="pt-2">
            {user?.role === "admin" && client?.id && client?.snapshot && client?.writeback && (
              <Button
                type="button"
                variant="destructive"
                disabled={deletingWriteback || client.status?.toLowerCase() === "online"}
                onClick={handleDeleteWriteback}
              >
                {deletingWriteback ? <Spinner data-icon="inline-start" /> : <Trash2 data-icon="inline-start" />}
                {deletingWriteback ? "Deleting..." : "Delete Writeback"}
              </Button>
            )}
            <Button type="button" variant="ghost" disabled={deletingWriteback} onClick={() => onClose()}>
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={deletingWriteback || isSubmitting || !isDirty || !isValid}
            >
              {isSubmitting ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <Save data-icon="inline-start" />
              )}
              {preparingWindowsDrivers ? "Preparing Windows drivers..." : isSubmitting ? "Saving..." : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default ClientFormModal;
