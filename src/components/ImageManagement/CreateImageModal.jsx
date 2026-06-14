import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Modal } from "@/components/ui";
import { createImage } from "@/api/modules/images";
import { imageSchema } from "@/schema";



const CreateImageModal = ({
  openImageCreateModal,
  setOpenImageCreateModal,
}) => {
  const fetchMasters = useAppStore((state) => state.fetchMasters);
  const { success, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset
  } = useForm({
    resolver: zodResolver(imageSchema),
    defaultValues: {
      name: "",
      size_gb: 50,
      os_type: "windows",
    },
  });

  const onSubmit = async (data) => {
    // Get token from localStorage
    try {
      await createImage(data);
      success("Image Management", `Image ${data.name} created successfully`);
      await fetchMasters(); // Refresh images
      reset();
      setOpenImageCreateModal(false);
    } catch (err) {
      error(
        "Image Management",
        `Failed to create image: ${err || "An unknown error occurred"}`
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
          <legend htmlFor="os_type" className="fieldset-legend">
            Operating System
          </legend>
          <select {...register("os_type")} id="os_type" className="select w-full">
            <option value="windows">Windows</option>
            <option value="linux">Linux</option>
          </select>
          {errors.os_type && (
            <div className="text-red-500 text-xs">{errors.os_type.message}</div>
          )}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor="size" className="fieldset-legend">
            Image Size (in GB)
          </legend>
          <input
            {...register("size_gb")}
            type="number"
            id="size_gb"
            placeholder="e.g., 50, 100, 1000"
            className="input w-full"
            title="Enter size (e.g., 50, 100, 1000)"
            min="1"
          />
          {errors.size_gb && (
            <div className="text-red-500 text-xs">{errors.size_gb.message}</div>
          )}
        </fieldset>
        <div className="mt-6 flex justify-end space-x-3">
          <Button
            type="submit"
            variant="primary"
            icon={Save}
            disabled={isSubmitting}
          >
            {isSubmitting ? "Creating..." : "Create Image"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => setOpenImageCreateModal(false)}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateImageModal;
