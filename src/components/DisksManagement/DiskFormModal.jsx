import { NativeSelectOption } from "@/components/ui/native-select";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { Button, Input, Modal, Select } from "@/components/ui";
import { DialogFooter } from "@/components/ui/dialog";

const diskSchema = z
  .object({
    zpool: z.string().min(1, "Zpool is required"),
    name: z.string().min(4, "Disk name is required"),
    usage_type: z.enum(["image", "writeback", "game"]),
    size: z.string().optional(),
  })
  .superRefine((value, context) => {
    if (value.usage_type === "game") {
      if (!value.size?.trim()) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["size"],
          message: "Game disk size is required",
        });
      } else if (!/^\d+(\.\d+)?[KMGTPE]?$/i.test(value.size.trim())) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["size"],
          message: "Size must look like 50G (number with optional K/M/G/T/P/E suffix)",
        });
      }
    }
  });

const DiskFormModal = ({ zpools, isOpen, setIsOpen, refresh }) => {
  const createDataset = useAppStore((state) => state.createDataset);
  const { success, error } = useToastStore();

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
    const result = await createDataset(data);
    if (result.success) {
      success("Disk Management", result.message);
      refresh();
      setIsOpen(false);
    } else {
      error("Disk Management", result.error);
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
          onChange={(e) =>
            setValue("zpool", e.target.value, {
              shouldValidate: true,
              shouldDirty: true,
            })
          }
          error={errors.zpool?.message}
        >
          <NativeSelectOption value="">Select zpool</NativeSelectOption>
          {zpools.map((p) => (
            <NativeSelectOption key={p} value={p}>
              {p}
            </NativeSelectOption>
          ))}
        </Select>

        <Select
          label="Disk type"
          register={register("usage_type")}
          onChange={(e) => {
            const next = e.target.value;
            setValue("usage_type", next, {
              shouldValidate: true,
              shouldDirty: true,
            });
            // A stale game size must not leak into other disk types.
            if (next !== "game") {
              setValue("size", "", {
                shouldValidate: true,
                shouldDirty: true,
              });
            }
          }}
          error={errors.usage_type?.message}
        >
          <NativeSelectOption value="">Select disk type</NativeSelectOption>
          <NativeSelectOption value="image">Image (store images)</NativeSelectOption>
          <NativeSelectOption value="writeback">Writeback (store clones)</NativeSelectOption>
          <NativeSelectOption value="game">Game (Game Disks - creates zvol)</NativeSelectOption>
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
        <DialogFooter className="mt-6">
          <Button type="submit" variant="primary" icon={Save}>
            {usageType === "game" ? "Create Game Disk" : "Create Master"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => setIsOpen(false)}
          >
            Cancel
          </Button>
        </DialogFooter>
      </form>
    </Modal>
  );
};

export default DiskFormModal;
