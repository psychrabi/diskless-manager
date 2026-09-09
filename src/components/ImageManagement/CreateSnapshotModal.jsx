import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { DialogFooter } from "@/components/ui/dialog";
import { Input as TextInput } from "@/components/ui/input";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "@/components/ui";
import { createSnapshot } from "@/api/modules/images";

const snapshotSchema = z.object({
  name: z.string().min(1, "Snapshot name is required"),
});

const CreateSnapshotModal = ({
  openSnapshotCreateModal,
  setOpenSnapshotCreateModal,
  selectedImage,
}) => {
  const fetchMasters = useAppStore((state) => state.fetchMasters);
  const { success, error } = useToastStore();
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset
  } = useForm({
    resolver: zodResolver(snapshotSchema),
  });

  const onSubmit = async (data) => {
    if (!selectedImage) return;
    try {
      await createSnapshot(selectedImage.id, data.name)
      success("Image Management", `Snapshot created successfully`);
      await fetchMasters();
      reset();
      setOpenSnapshotCreateModal(false);
    } catch (err) {
      error(
        `Failed to create snapshot: ${err || "An unknown error occurred"
        }`
      );
    }
  };

  return (
    <Modal
      isOpen={openSnapshotCreateModal}
      onClose={() => setOpenSnapshotCreateModal(false)}
      title={`Create Snapshot for ${selectedImage.name}`}
      size="2xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">
        <Field>
          <FieldLabel htmlFor="name">
            Snapshot Name
          </FieldLabel>
          <TextInput
            {...register("name")}
            type="text"
            id="name"
            placeholder="Enter snapshot name (e.g., my-snapshot-name)"
            className="w-full"
          />
          {errors.name && (
            <FieldError>{errors.name.message}</FieldError>
          )}
        </Field>
        <FieldDescription className="mt-2">
          This operation will capture the current state of{" "}
          <strong className="font-semibold">{selectedImage.name}</strong>.
        </FieldDescription>
        <DialogFooter className="mt-6">
          <Button
            type="submit"
            variant="primary"
            icon={Save}
            disabled={isSubmitting}
          >
            {isSubmitting ? "Creating..." : "Create Snapshot"}
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => setOpenSnapshotCreateModal(false)}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
        </DialogFooter>
      </form>
    </Modal>
  );
};

export default CreateSnapshotModal;
