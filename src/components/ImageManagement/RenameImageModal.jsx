import * as api from "@/api/commands";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "@/components/ui";

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

}) => {
  const fetchMasters = useAppStore((state) => state.fetchMasters);
  const { success, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(renameImageSchema),
    defaultValues: {
      newName: "",
    },
  });

  const onSubmit = async (data) => {
    if (!selectedImage) return;
    console.log(data)
    // Extract the base name from the full ZFS path (e.g., "diskless/win11-master" -> "win11")
    try {
      await api.renameImage(selectedImage.id, data.newName)
      success("Image Management", `Image renamed to ${data.newName}`);
      await fetchMasters();
      reset();
      setOpenRenameModal(false);
    } catch (err) {
      error(
        `Failed to rename image: ${err || "An unknown error occurred"
        }`
      );
    }
  };

  const handleClose = () => {
    setOpenRenameModal(false);
    reset();
  };


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
            Rename master image "{selectedImage.name}" to a new name.
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
          <Button
            type="submit"
            variant="primary"
            icon={Save}
            disabled={isSubmitting}
          >
            {isSubmitting ? "Renaming..." : "Rename Image"}
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={handleClose}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default RenameImageModal;
