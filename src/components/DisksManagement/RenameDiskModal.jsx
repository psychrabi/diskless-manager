import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { DialogFooter } from "@/components/ui/dialog";
import { Input as TextInput } from "@/components/ui/input";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { renameDisk } from "@/api/modules/disks";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "@/components/ui";

const renameDiskSchema = z.object({
  newName: z
    .string()
    .min(1, "New name is required")
    .regex(
      /^[\w-]+$/,
      "Name can only contain alphanumeric characters, underscores, and hyphens"
    ),
});

const RenameDiskModal = ({
  openRenameModal,
  setOpenRenameModal,
  selectedDisk,
  refresh,
}) => {
  const { success, info, error } = useToastStore();
  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(renameDiskSchema),
    defaultValues: {
      newName: "",
    },
  });

  const onSubmit = async (data) => {
    if (!selectedDisk) return;

    // Extract the dataset name from the full path (e.g., "tank/images/ubuntu" -> "ubuntu")
    const baseName = selectedDisk.name
      ? selectedDisk.name.split("/").pop()
      : "";
    info(`Renaming disk from ${baseName} to ${data.newName}`);
    setOpenRenameModal(false);

    try {
      const response = await renameDisk(selectedDisk.name, data.newName);
      if (response.message) {
        success("Disk Management", response.message);
      } else {
        success("Disk Management", `Successfully renamed disk to ${data.newName}`);
      }
      reset();
    } catch (err) {
      error(
        "Disk Management",
        `Failed to rename disk: ${err.message || "An unknown error occurred"}`
      );
    } finally {
      refresh && refresh();
    }
  };

  const handleClose = () => {
    setOpenRenameModal(false);
    reset();
  };

  // Extract display name from the full ZFS path (e.g., "tank/images/ubuntu" -> "ubuntu")
  const displayName = selectedDisk?.name
    ? selectedDisk.name.split("/").pop()
    : "";

  return (
    <Modal
      isOpen={openRenameModal}
      onClose={handleClose}
      title="Rename disk"
      size="xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div className="space-y-2">
          <FieldDescription>
            Rename disk &quot;{displayName}&quot; to a new name.
          </FieldDescription>
          <Field>
            <FieldLabel htmlFor="newName">New Name</FieldLabel>
            <TextInput
              {...register("newName")}
              type="text"
              id="newName"
              placeholder="e.g., boot-disk, writeback-disk"
              className="w-full"
              aria-invalid={!!errors.newName}
              aria-describedby={errors.newName ? "newName-error" : undefined}
            />
            {errors.newName && (
              <FieldError id="newName-error">
                {errors.newName.message}
              </FieldError>
            )}
          </Field>
        </div>
        <DialogFooter className="mt-6">
          <Button type="submit" variant="primary" icon={Save}>
            Rename disk
          </Button>
          <Button type="button" variant="destructive" onClick={handleClose}>
            Cancel
          </Button>
        </DialogFooter>
      </form>
    </Modal>
  );
};

export default RenameDiskModal;
