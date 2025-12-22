import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { invoke } from "@tauri-apps/api/core";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Modal } from "../ui";

const imageSchema = z.object({
  name: z.string().min(1, "Image name is required"),
  size: z.string().min(1, "Image Size is required"),
  os: z.string().optional(),
});

const CreateImageModal = ({
  openImageCreateModal,
  setOpenImageCreateModal,
}) => {
  const fetchImages = useAppStore((state) => state.fetchImages);
  const { success, info, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(imageSchema),
    defaultValues: {
      name: "",
      size: "50G",
      os: "windows",
    },
  });

  const onSubmit = async (data) => {
    info(`Adding new ZFS image ${data.name}`);

    setOpenImageCreateModal(false);

    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    try {
      const response = await invoke("create_image", {
        request: { token, name: data.name, size: data.size, os: data.os },
      });
      if (response.message) success(response.message);
      fetchImages(); // Refresh images
    } catch (err) {
      error(
        `Failed to create image: ${
          err.message || "An unknown error occurred"
        }`,
      );
    }
  };

  return (
    <Modal
      isOpen={openImageCreateModal}
      onClose={() => setOpenImageCreateModal(false)}
      title="Create Image"
      size="xl"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <fieldset className={`fieldset`}>
          <legend htmlFor="name" className="fieldset-legend">
            Image Name
          </legend>
          <input
            {...register("name")}
            type="text"
            id="name"
            placeholder="e.g., win11-enterprise (will create pool/name-master)"
            className="input w-full"
          />
          {errors.name && (
            <div className="text-red-500 text-xs">{errors.name.message}</div>
          )}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor="os" className="fieldset-legend">
            Operating System
          </legend>
          <select {...register("os")} id="os" className="select w-full">
            <option value="windows">Windows</option>
            <option value="linux">Linux</option>
          </select>
          {errors.os && (
            <div className="text-red-500 text-xs">{errors.os.message}</div>
          )}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor="size" className="fieldset-legend">
            Image Size
          </legend>
          <input
            {...register("size")}
            type="text"
            id="size"
            placeholder="e.g., 50G, 1T"
            className="input w-full"
            title="Enter size (e.g., 50G, 100G, 1T)"
          />
          {errors.size && (
            <div className="text-red-500 text-xs">{errors.size.message}</div>
          )}
        </fieldset>
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>
            Create Image
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => setOpenImageCreateModal(false)}
          >
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateImageModal;
