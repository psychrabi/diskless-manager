import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "../ui";

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

    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    await invoke("rename_zfs_dataset", {
      token,
      old: selectedDisk.name,
      new: data.newName,
    })
      .then((response) => {
        if (response.message) success("Disk Management", response.message);
        reset();
      })
      .catch((err) => {
        error(
          "Disk Management",
          `Failed to rename disk: ${err.message || "An unknown error occurred"}`
        );
      })
      .finally(() => {
        refresh && refresh();
      });
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
          <p className="text-sm text-gray-600 dark:text-gray-400">
            Rename disk "{displayName}" to a new name.
          </p>
          <fieldset className={`fieldset`}>
            <legend className="fieldset-legend">New Name</legend>
            <input
              {...register("newName")}
              type="text"
              id="newName"
              placeholder="e.g., boot-disk, writeback-disk"
              className="input w-full"
              aria-invalid={!!errors.newName}
              aria-describedby={errors.newName ? "newName-error" : undefined}
            />
            {errors.newName && (
              <div
                id="newName-error"
                role="alert"
                className="text-red-500 text-xs"
              >
                {errors.newName.message}
              </div>
            )}
          </fieldset>
        </div>
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            Rename disk
          </Button>
          <Button type="button" variant="destructive" onClick={handleClose}>
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default RenameDiskModal;
