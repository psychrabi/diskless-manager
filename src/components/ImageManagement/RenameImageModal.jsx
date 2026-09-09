import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { DialogFooter } from "@/components/ui/dialog";
import { Input as TextInput } from "@/components/ui/input";
import { renameImage } from "@/api/modules/images";
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
      await renameImage(selectedImage.id, data.newName)
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
          <FieldDescription>
            Rename master image &quot;{selectedImage.name}&quot; to a new name.
          </FieldDescription>
          <Field>
            <FieldLabel htmlFor="newName">
              New Name
            </FieldLabel>
            <TextInput
              {...register("newName")}
              type="text"
              id="newName"
              placeholder="e.g., win11-enterprise (will create pool/name-master)"
              className="w-full"
            />
            {errors.newName && (
              <FieldError>{errors.newName.message}</FieldError>
            )}
          </Field>
        </div>

        <DialogFooter className="mt-6">
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
        </DialogFooter>
      </form>
    </Modal>
  );
};

export default RenameImageModal;
