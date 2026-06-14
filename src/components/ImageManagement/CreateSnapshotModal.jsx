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
        <fieldset className={`fieldset`}>
          <legend htmlFor="name" className="fieldset-legend">
            Snapshot Name
          </legend>
          <input
            {...register("name")}
            type="text"
            id="name"
            placeholder="Enter snapshot name (e.g., my-snapshot-name)"
            className="input w-full"
          />
          {errors.name && (
            <div className="text-red-500 text-xs">{errors.name.message}</div>
          )}
        </fieldset>
        <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
          This operation will capture the current state of{" "}
          <strong className="font-semibold">{selectedImage.name}</strong>.
        </p>
        <div className="mt-6 flex justify-end space-x-3">
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
        </div>
      </form>
    </Modal>
  );
};

export default CreateSnapshotModal;
