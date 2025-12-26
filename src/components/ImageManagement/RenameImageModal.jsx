import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "../ui";

const renameImageSchema = z.object({
  newName: z
    .string()
    .min(1, "New name is required")
    .regex(
      /^[\w-]+$/,
      "Name can only contain alphanumeric characters, underscores, and hyphens"
    ),
});

const RenameImageModal = ({
  openRenameModal,
  setOpenRenameModal,
  selectedImage,
  refresh,
}) => {
  const fetchImages = useAppStore((state) => state.fetchImages);
  const { success, info, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(renameImageSchema),
    defaultValues: {
      newName: "",
    },
  });

  const onSubmit = async (data) => {
    if (!selectedImage) return;

    // Extract the base name from the full ZFS path (e.g., "diskless/win11-master" -> "win11")
    const baseName =
      selectedImage.split("/").pop()?.replace("-master", "") || "";

    info(`Renaming image from ${baseName} to ${data.newName}`);

    setOpenRenameModal(false);

    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    await invoke("rename_image", {
      token,
      oldName: selectedImage,
      newName: data.newName,
    })
      .then(async (response) => {
        if (response.message) success(response.message);
        await fetchImages();
        reset();
      })
      .catch((error) => {
        error(
          `Failed to rename image: ${
            error.message || "An unknown error occurred"
          }`
        );
      });
  };

  const handleClose = () => {
    setOpenRenameModal(false);
    reset();
  };

  // Extract the base name from the full ZFS path for display
  const displayName = selectedImage
    ? selectedImage.split("/").pop()?.replace("-master", "") || selectedImage
    : "";

  return (
    <Modal
      isOpen={openRenameModal}
      onClose={handleClose}
      title="Rename Master Image"
      size="xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div className="space-y-2">
          <p className="text-sm text-gray-600 dark:text-gray-400">
            Rename master image "{displayName}" to a new name.
          </p>
          <fieldset className={`fieldset`}>
            <legend htmlFor="newName" className="fieldset-legend">
              New Name
            </legend>
            <input
              {...register("newName")}
              type="text"
              id="newName"
              placeholder="e.g., win11-enterprise (will create pool/name-master)"
              className="input w-full"
            />
            {errors.newName && (
              <div className="text-red-500 text-xs">
                {errors.newName.message}
              </div>
            )}
          </fieldset>
        </div>

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            Rename Image
          </Button>
          <Button type="button" variant="destructive" onClick={handleClose}>
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default RenameImageModal;
