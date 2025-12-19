import { useNotification } from "@/contexts/notification";
import { useZfs } from "@/hooks/useZfs";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { Button, Input, Modal, Select } from "../ui";

const diskSchema = z.object({
  zpool: z.string().min(1, "Zpool is required"),
  name: z.string().min(4, "Disk name is required"),
  usage_type: z.string().min(1, "Disk type is required"),
  size: z.string().optional(),
});

const DiskFormModal = ({ zpools, isOpen, setIsOpen, refresh }) => {
  const { showNotification } = useNotification();
  const { createDataset } = useZfs();

  const defaultValues = { zpool: "", name: "", usage_type: "image", size: "" };

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    control,
  } = useForm({
    resolver: zodResolver(diskSchema),
    defaultValues,
  });

  const onSubmit = async (data) => {
    showNotification(`Adding new disk ${data.name}`, "info");
    const success = await createDataset(data);
    if (success) {
      refresh();
      setIsOpen(false);
    }
  };

  const usageType = useWatch({
    control,
    name: "usage_type",
  });

  return (
    <Modal
      isOpen={isOpen}
      onClose={() => setIsOpen(false)}
      title="Add disk"
      size="xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Select
          label="Select zpool"
          register={register("zpool")}
          onChange={(e) => setValue("zpool", e.target.value)}
          error={errors.zpool?.message}
        >
          <option value="">Select zpool</option>
          {zpools.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </Select>

        <Select
          label="Disk type"
          register={register("usage_type")}
          onChange={(e) => setValue("usage_type", e.target.value)}
          error={errors.usage_type?.message}
        >
          <option value="">Select disk type</option>
          <option value="image">Image (store images)</option>
          <option value="writeback">Writeback (store clones)</option>
          <option value="game">Game (Game Disks - creates zvol)</option>
        </Select>

        <Input
          label="Disk Name"
          register={register("name")}
          type="text"
          placeholder="Enter disk name"
          error={errors.name?.message}
        />

        {usageType === "game" && (
          <Input
            label="Disk size"
            register={register("size")}
            type="text"
            placeholder="e.g. 50G"
            error={errors.size?.message}
          />
        )}
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            Create Master
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

export default DiskFormModal;
