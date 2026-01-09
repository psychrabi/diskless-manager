import { useSettings } from "@/hooks/useSettings";
import { tftpSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import TFTPForm from "./Forms/TFTPForm";

export default function TFTPConfigForm() {
  const { info } = useToastStore();
  const { updateTftp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.tftp || {},
    resolver: zodResolver(tftpSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating TFTP Configurations`);
    await updateTftp(data);
  };

  return (
    <Card title="TFTP Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <TFTPForm
          register={register}
          errors={errors}
          config={config?.settings?.tftp}
        />

        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save TFTP Settings"}
        </Button>
      </form>
    </Card>
  );
}
