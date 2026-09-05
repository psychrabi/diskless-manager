import {
  addClient,
  getClientNvmeOfStatus,
  prepareClientNvmeOf,
  removeClientNvmeOf,
  updateClient,
} from "@/api/modules/clients";
import { readConfig } from "@/api/modules/config";
import { clientSchema } from "@/schema";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { RefreshCw, Save, Server, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useForm, useWatch } from "react-hook-form";
import { Button, Input, Modal, Select } from "@/components/ui";

// Each opened client owns its form and asynchronous status state. Responses
// belonging to an unmounted client cannot overwrite another client's dialog.
const ClientFormModal = (props) => (
  <ClientFormModalContent key={`${props.client?.id ?? "new"}:${props.isOpen}`} {...props} />
);

const ClientFormModalContent = ({ client, masters, isOpen, onClose, refresh }) => {
  const { success, error } = useToastStore();
  const [nvmeStatus, setNvmeStatus] = useState(null);
  const [nvmeLoading, setNvmeLoading] = useState(Boolean(isOpen && client?.id));
  const [nvmeAction, setNvmeAction] = useState(null);
  const [nvmeError, setNvmeError] = useState("");

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting, isDirty, isValid },
    setValue,
    control,
    reset,
  } = useForm({
    mode: "onChange",
    resolver: zodResolver(clientSchema),
    defaultValues: client,
  });

  useEffect(() => {
    if (isOpen && client) {
      reset(client || {});
    }
  }, [client, isOpen, reset]);

  const loadNvmeStatus = useCallback(async () => {
    if (!isOpen || !client?.id) {
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
  }, [client, isOpen]);

  useEffect(() => {
    if (!isOpen || !client?.id) return;
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
  }, [client?.id, isOpen]);

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
    if (!client?.id) return;

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
    try {
      const mode = !data.snapshot ? "super" : data.mode || "normal";

      if (!client?.id) {
        await addClient({
          name: data.name,
          mac: data.mac,
          ip: data.ip,
          master: data.master,
          snapshot: data.snapshot || null,
          mode: mode,
          keep_writeback: data.keep_writeback,
          use_game_disk: data.use_game_disk,
        });
        success("Client Management", `Client ${data.name} added successfully.`);
      } else {
        await updateClient(client.id, {
          name: data.name,
          mac: data.mac,
          ip: data.ip,
          master: data.master,
          snapshot: data.snapshot || null,
          mode: mode,
          keep_writeback: data.keep_writeback,
          use_game_disk: data.use_game_disk,
        });
        success(
          "Client Management",
          `Client ${data.name} updated successfully.`
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

  const selectedSnapshot = useWatch({
    control,
    name: "snapshot",
  });

  useEffect(() => {
    if (!selectedSnapshot) {
      setValue("mode", "super", { shouldValidate: true });
    }
  }, [selectedSnapshot, setValue]);

  const nvmeReady = Boolean(
    nvmeStatus?.subsystem_present &&
      nvmeStatus?.namespace_enabled &&
      nvmeStatus?.port_attached
  );

  return (
    <Modal
      isOpen={isOpen}
      onClose={() => onClose()}
      title={client?.id ? "Edit Client" : "Add Client"}
      size="xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          label="Client Name"
          register={register("name")}
          type="text"
          placeholder="enter client name"
          error={errors.name?.message}
        />
        <Input
          label="MAC Address"
          register={register("mac")}
          type="text"
          placeholder="XX:XX:XX:XX:XX:XX"
          error={errors.mac?.message}
        />
        <Input
          label="IP Address"
          register={register("ip")}
          type="text"
          placeholder="X.X.X.X"
          error={errors.ip?.message}
        />
        <Select
          label="Select Image"
          register={register("master")}
          onChange={() => {
            setValue("snapshot", "", {
              shouldValidate: true,
              shouldDirty: true,
            });
          }}
          error={errors.master?.message}
        >
          <option value="">Select image ...</option>
          {masters?.map((master) => (
            <option key={master.name} value={master.name}>
              {master.name}
            </option>
          ))}
        </Select>
        <Select
          label="Select Snapshot"
          register={register("snapshot")}
          disabled={!selectedMaster}
          error={errors.snapshot?.message}
        >
          <option value="">Use master directly</option>
          {masters?.find((m) => m.name === selectedMaster)
            ?.snapshots?.map((snap) => (
              <option key={snap.name} value={`${selectedMaster}@${snap.name}`}>
                {snap.name} ({snap.created}, {snap.size})
              </option>
            ))}
        </Select>

        <div className="form-control">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              {...register("keep_writeback")}
            />
            <div className="flex flex-col">
              <span className="label-text font-medium">
                Keep Writeback (Persistent Mode)
              </span>
              <span className="label-text-alt text-base-content/60 text-wrap text-xs">
                If unchecked, the clone resets after the offline delay configured in Settings
                (non-persistent mode)
              </span>
            </div>
          </label>
        </div>

        <div className="form-control">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              {...register("use_game_disk")}
            />
            <div className="flex flex-col">
              <span className="label-text font-medium">Use Game Disk</span>
              <span className="label-text-alt text-base-content/60 text-wrap text-xs">
                If checked, available game disks will be mounted via iSCSI
              </span>
            </div>
          </label>
        </div>

        {client?.id && (
          <div className="rounded-lg border border-warning/30 bg-warning/5 p-4">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <div className="flex items-center gap-2 font-semibold">
                  <Server className="h-4 w-4" />
                  Experimental NVMe/TCP
                </div>
                <p className="mt-1 text-xs text-base-content/60">
                  Exposes this client's existing ZVOL through Linux NVMe/TCP.
                  iSCSI remains the normal/default boot path.
                </p>
              </div>
              <span
                className={`badge ${
                  nvmeReady
                    ? "badge-success"
                    : nvmeLoading
                      ? "badge-ghost"
                      : "badge-warning"
                }`}
              >
                {nvmeLoading ? "Checking..." : nvmeReady ? "Ready" : "Not ready"}
              </span>
            </div>

            <div className="grid gap-2 text-sm sm:grid-cols-2">
              <div>
                <span className="text-base-content/60">NQN:</span>
                <div className="break-all font-mono text-xs">
                  {nvmeStatus?.nqn || "—"}
                </div>
              </div>
              <div>
                <span className="text-base-content/60">Block device:</span>
                <div className="break-all font-mono text-xs">
                  {nvmeStatus?.block_device || client?.block_device || client?.block_store || "—"}
                </div>
              </div>
              <div>
                <span className="text-base-content/60">Namespace:</span>{" "}
                <span>{nvmeStatus?.namespace_enabled ? "Enabled" : "Disabled"}</span>
              </div>
              <div>
                <span className="text-base-content/60">TCP port:</span>{" "}
                <span>{nvmeStatus?.tcp_port || 4420}</span>
              </div>
              <div>
                <span className="text-base-content/60">Subsystem:</span>{" "}
                <span>{nvmeStatus?.subsystem_present ? "Present" : "Missing"}</span>
              </div>
              <div>
                <span className="text-base-content/60">Port attached:</span>{" "}
                <span>{nvmeStatus?.port_attached ? "Yes" : "No"}</span>
              </div>
            </div>

            {nvmeError && (
              <div className="alert alert-error mt-3 py-2 text-xs">
                <span>{nvmeError}</span>
              </div>
            )}

            <div className="mt-4 flex flex-wrap gap-2">
              <Button
                type="button"
                variant="primary"
                icon={Server}
                disabled={Boolean(nvmeAction)}
                onClick={handlePrepareNvme}
              >
                {nvmeAction === "prepare"
                  ? "Preparing..."
                  : nvmeReady
                    ? "Refresh NVMe/TCP Target"
                    : "Prepare NVMe/TCP Target"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                icon={RefreshCw}
                disabled={Boolean(nvmeAction) || nvmeLoading}
                onClick={loadNvmeStatus}
              >
                Refresh Status
              </Button>
              <Button
                type="button"
                variant="ghost"
                icon={Trash2}
                disabled={Boolean(nvmeAction) || !nvmeStatus?.subsystem_present}
                onClick={handleRemoveNvme}
              >
                {nvmeAction === "remove" ? "Removing..." : "Remove NVMe Target"}
              </Button>
            </div>
          </div>
        )}

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="button" variant="ghost" onClick={() => onClose()}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            icon={Save}
            disabled={isSubmitting || !isDirty || !isValid}
          >
            {isSubmitting ? "Saving..." : "Save"}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;
