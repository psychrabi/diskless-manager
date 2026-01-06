import { createImage } from "@/api/commands";
import { imageSchema } from "@/schema";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { Input, Select } from "../ui";
import { Modal } from "../ui/Modal";

export default function CreateImageForm({ show, onClose }) {
  const { success, error } = useToastStore();
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    resolver: zodResolver(imageSchema),
  });

  async function handleCreate(data) {
    try {
      await createImage(data);
      success("Image Management", `Image "${data.name}" created successfully`);
      onClose(false);
    } catch (e) {
      error("Image Management", `Failed to create image: ${e}`);
    }
  }
  return (
    <Modal
      title="Create New Image"
      isOpen={show}
      onClose={() => onClose(false)}
    >
      <form className="space-y-4" onSubmit={handleSubmit(handleCreate)}>
        <Input
          label="Name"
          type="text"
          register={register("name")}
          placeholder="ubuntu-22.04"
          required
          error={errors.name?.message}
        />
        <Select
          label="Operating System"
          register={register("os_type")}
          required
          error={errors.os_type?.message}
        >
          <option value="linux">Linux</option>
          <option value="windows">Windows</option>
        </Select>

        <Select
          label="Format"
          register={register("format")}
          required
          error={errors.format?.message}
        >
          <option value="raw">RAW</option>
          <option value="qcow2">QCOW2</option>
        </Select>

        <Input
          label="Size (GB)"
          type="text"
          register={register("size_gb")}
          placeholder="50"
          required
          error={errors.size_gb?.message}
        />

        <Input
          label="Description"
          type="textarea"
          register={register("description")}
          placeholder="Optional description..."
          error={errors.description?.message}
        />

        <div className="flex justify-end gap-3 pt-4 mt-2 border-t border-base-200">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => onClose(false)}
            disabled={isSubmitting}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={isSubmitting}
          >
            {isSubmitting && (
              <span className="loading loading-spinner loading-sm"></span>
            )}
            Create Image
          </button>
        </div>
      </form>
    </Modal>
  );
}
