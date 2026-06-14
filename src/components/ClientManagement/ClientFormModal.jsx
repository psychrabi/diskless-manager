import { addClient, updateClient } from "@/api/modules/clients";
import { clientSchema } from "@/schema";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useEffect } from "react";
import { useForm, useWatch } from "react-hook-form";
import { Button, Input, Modal, Select } from "@/components/ui";

const ClientFormModal = ({ client, masters, isOpen, onClose, refresh }) => {
  const { success, error } = useToastStore();

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

  // const formValues = watch();

  // Reset form when client changes
  useEffect(() => {
    if (isOpen && client) {
      reset(client || {});
    }
  }, [client, isOpen, reset]);

  const onSubmit = async (data) => {
    try {
      // If using master directly (no snapshot), force super client mode
      const mode = !data.snapshot ? "super" : data.mode || "normal";

      if (!client?.id) {
        // Create new client
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
        // Update existing client
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
      reset(); // Reset to clear form
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

  // Auto-set mode to "super" when using master directly (no snapshot)
  useEffect(() => {
    if (!selectedSnapshot) {
      setValue("mode", "super", { shouldValidate: true });
    }
  }, [selectedSnapshot, setValue]);

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

        {/* Keep Writeback Checkbox */}
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
            />
            <div className="flex flex-col">
              <span className="label-text font-medium">Use Game Disk</span>
              <span className="label-text-alt text-base-content/60 text-wrap text-xs">
                If checked, available game disks will be mounted via iSCSI
              </span>
            </div>
          </label>
        </div>

        <div className="mt-6 flex justify-end space-x-3">
          <Button
            type="button"
            variant="ghost"
            onClick={() => onClose()}
          >
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
