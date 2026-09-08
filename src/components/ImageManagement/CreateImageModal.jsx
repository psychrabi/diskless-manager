import { Field, FieldLabel } from "@/components/ui/field";
import { Input as TextInput } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
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
        <Field>
          <FieldLabel htmlFor="name">
            Image Name
          </FieldLabel>
          <TextInput
            {...register("name")}
            type="text"
            id="name"
            placeholder="e.g., win11-enterprise (will create pool/name-master)"
            className="w-full"
          />
          {errors.name && (
            <div className="text-destructive text-xs">{errors.name.message}</div>
          )}
        </Field>

        <Field>
          <FieldLabel htmlFor="os_type">
            Operating System
          </FieldLabel>
          <NativeSelect {...register("os_type")} id="os_type" className="w-full">
            <NativeSelectOption value="windows">Windows</NativeSelectOption>
            <NativeSelectOption value="linux">Linux</NativeSelectOption>
          </NativeSelect>
          {errors.os_type && (
            <div className="text-destructive text-xs">{errors.os_type.message}</div>
          )}
        </Field>

        <Field>
          <FieldLabel htmlFor="size_gb">
            Image Size (in GB)
          </FieldLabel>
          <TextInput
            {...register("size_gb")}
            type="number"
            id="size_gb"
            placeholder="e.g., 50, 100, 1000"
            className="w-full"
            title="Enter size (e.g., 50, 100, 1000)"
            min="1"
          />
          {errors.size_gb && (
            <div className="text-destructive text-xs">{errors.size_gb.message}</div>
          )}
        </Field>
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
