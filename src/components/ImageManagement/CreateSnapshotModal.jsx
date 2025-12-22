import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "../ui";

const snapshotSchema = z.object({
  name: z.string().min(1, "Snapshot name is required"),
});

const CreateSnapshotModal = ({
  openSnapshotCreateModal,
  setOpenSnapshotCreateModal,
  selectedImage,
}) => {
  const { success, info, error } = useToastStore();
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(snapshotSchema),
    defaultValues: {
      name: "",
      size: "50G",
    },
  });

  const onSubmit = async (data) => {
    const fullSnapshotName = `${selectedImage}@${data.name}`;
    info(`Adding new snapshot ${fullSnapshotName}`);
    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    await invoke("create_snapshot", {
      token,
      masterName: selectedImage,
      snapshotName: fullSnapshotName,
    })
      .then((response) => {
        if (response.message) success(response.message);
      })
      .catch((error) => {
        error(
          `Failed to create snapshot: ${
            error.message || "An unknown error occurred"
          }`,
        );
      });
    setOpenSnapshotCreateModal(false);
  };

  return (
    <Modal
      isOpen={openSnapshotCreateModal}
      onClose={() => setOpenSnapshotCreateModal(false)}
      title={`Create Snapshot for ${selectedImage}`}
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
          <strong className="font-semibold">{selectedImage}</strong>.
        </p>
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            Create Snapshot
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => setOpenSnapshotCreateModal(false)}
          >
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateSnapshotModal;
