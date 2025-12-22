import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Save } from "lucide-react";
import { useEffect } from "react";
import { useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { useToastStore } from "@/store/useToastStore";
import { Button, Input, Modal, Select } from "../ui";

const clientSchema = z.object({
  name: z.string().min(1, "Client name is required"),
  mac: z
    .string()
    .regex(
      /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/,
      "Invalid MAC address format",
    ),
  ip: z
    .string()
    .regex(
      /^([\d]{1,3}\.){3}\d{1,3}$/,
      "Invalid IP address format. Use X.X.X.X",
    ),
  master: z.string().optional(),
  snapshot: z.string().optional().nullable(),
  keep_writeback: z.boolean().optional(),
  use_game_disk: z.boolean().optional(),
});

const ClientFormModal = ({ client, masters, isOpen, setIsOpen, refresh }) => {
  const { success, info } = useToastStore();

  const defaultValues = {
    name: client?.name || "",
    mac: client?.mac || "",
    ip: client?.ip || "",
    master: client?.master || "",
    snapshot: client?.snapshot || null,
    pxe_mode: client?.pxe_mode || "uefi",
    keep_writeback: client?.keep_writeback !== false, // Default to true
    use_game_disk: client?.use_game_disk || false,
  };

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    control,
    reset,
  } = useForm({
    resolver: zodResolver(clientSchema),
    defaultValues,
  });

  // Reset form when client changes
  useEffect(() => {
    if (isOpen && client) {
      reset({
        name: client.name || "",
        mac: client.mac || "",
        ip: client.ip || "",
        master: client.master || "",
        snapshot: client.snapshot || null,
        pxe_mode: client.pxe_mode || "uefi",
        keep_writeback: client.keep_writeback !== false,
        use_game_disk: client.use_game_disk || false,
      });
    }
  }, [client, isOpen, reset]);

  const onSubmit = async (data) => {
    const token = localStorage.getItem("authToken") || "";
    try {
      if (!client.id) {
        info(`Adding new client ${data.name}`);
        await invoke("add_client", { token, req: data });
        success(`Client ${data.name} added successfully.`);
      } else {
        info(`Editing client ${data.name}`);
        await invoke("edit_client", {
          token,
          clientId: client.id,
          data: {
            name: data.name,
            mac: data.mac,
            ip: data.ip,
            master: data.master,
            snapshot: data.snapshot || null,
            pxe_mode: data.pxe_mode,
            keep_writeback: data.keep_writeback,
            use_game_disk: data.use_game_disk,
          },
        });
        success(`Client ${data.name} updated successfully.`);
      }
      setIsOpen(false);
      reset(defaultValues);
      await refresh();
    } catch (e) {
      // Let existing error handling via toasts/console from invoke callers surface
      console.error("Failed to submit client form", e);
    }
  };

  const selectedMaster = useWatch({
    control,
    name: "master",
  });

  useEffect(() => {
    if (!client?.id || selectedMaster !== client?.master) {
      setValue("snapshot", "");
    }
  }, [client?.id, client?.master, selectedMaster, setValue]);

  return (
    <Modal
      isOpen={isOpen}
      onClose={() => setIsOpen(false)}
      title={client?.id ? "Edit Client" : "Create Client"}
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
          onChange={(e) => setValue("master", e.target.value)}
          error={errors.master?.message}
        >
          <option value="">Select image ...</option>
          {masters.map((master) => (
            <option key={master.name} value={master.name}>
              {master.name}
            </option>
          ))}
        </Select>
        <Select
          label="Select Snapshot"
          register={register("snapshot")}
          disabled={!selectedMaster}
          onChange={(e) => setValue("snapshot", e.target.value)}
          error={errors.snapshot?.message}
        >
          <option value="">Use master directly</option>
          {masters
            .find((m) => m.name === selectedMaster)
            ?.snapshots?.map((snap) => (
              <option key={snap.name} value={snap.name}>
                {snap.name} ({snap.created}, {snap.size})
              </option>
            ))}
        </Select>

        {/* Keep Writeback Checkbox */}
        <div className="form-control">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              {...register("keep_writeback")}
              defaultChecked={client?.keep_writeback !== false}
            />
            <div className="flex flex-col">
              <span className="label-text font-medium">
                Keep Changes (Persistent Mode)
              </span>
              <span className="label-text-alt text-base-content/60">
                If unchecked, client will reset to clean state on every boot
                (non-persistent mode)
              </span>
            </div>
          </label>
        </div>

        {/* Use Game Disk Checkbox */}
        <div className="form-control">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              {...register("use_game_disk")}
              defaultChecked={client?.use_game_disk}
            />
            <div className="flex flex-col">
              <span className="label-text font-medium">Use Game Disk</span>
              <span className="label-text-alt text-base-content/60">
                If checked, available game disks will be mounted via iSCSI
              </span>
            </div>
          </label>
        </div>

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            {client?.id ? "Update Client" : "Create Client"}
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => setIsOpen(false)}
          >
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;
