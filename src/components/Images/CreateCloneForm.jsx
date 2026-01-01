import { cloneImage } from "@/api/commands";
import { cloneSchema } from "@/schema";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { Input, Modal } from "../ui";

export default function CreateCloneForm({ show, onClose, selectedImage }) {
  const { success, error } = useToastStore();
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    resolver: zodResolver(cloneSchema),
  });

  async function handleClone(data) {
    try {
      await cloneImage(selectedImage.id, data.name);
      success(`Image cloned as "${data.name}"`);
      onClose(false);
    } catch (e) {
      error(`Failed to clone image: ${e}`);
    }
  }

  return (
    <Modal title="Clone Image" isOpen={show} onClose={() => onClose(false)}>
      <form className="p-6 space-y-4" onSubmit={handleSubmit(handleClone)}>
        {selectedImage && (
          <div className="alert role-alert">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              className="stroke-info shrink-0 w-6 h-6"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="2"
                d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              ></path>
            </svg>
            <span>
              Cloning <strong>{selectedImage.name}</strong>
            </span>
          </div>
        )}

        <Input
          label="New Image Name"
          type="text"
          register={register("name")}
          placeholder="ubuntu-22.04"
          required
          error={errors.name?.message}
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
            Clone Image
          </button>
        </div>
      </form>
    </Modal>
  );
}
