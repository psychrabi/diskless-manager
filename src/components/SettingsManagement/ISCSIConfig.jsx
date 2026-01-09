import { useSettings } from "@/hooks/useSettings";
import { iscsiSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import ISCSIForm from "./Forms/ISCSIForm";
import { Button, Card } from "@/components/ui";
import { Network } from "lucide-react";

export default function ISCSIConfig() {
  const { info } = useToastStore();
  const { updateIscsi } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.iscsi || {},
    resolver: zodResolver(iscsiSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating ISCSI Configurations`);
    await updateIscsi(data);
  };
  return (
    <Card title="ISCSI Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <ISCSIForm
          register={register}
          errors={errors}
          config={config?.settings?.iscsi}
        />

        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save ISCSI Settings"}
        </Button>
      </form>
    </Card>
  );
}
