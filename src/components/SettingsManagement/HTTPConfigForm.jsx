import { useSettings } from "@/hooks/useSettings";
import { httpSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import HTTPForm from "./Forms/HTTPForm";

export default function HTTPConfigForm() {
  const { info } = useToastStore();
  const { updateHttp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.http || {},
    resolver: zodResolver(httpSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating HTTP Configurations`);
    await updateHttp(data);
  };

  return (
    <Card title="HTTP Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <HTTPForm
          register={register}
          errors={errors}
          config={config?.settings?.http}
        />

        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save HTTP Settings"}
        </Button>
      </form>
    </Card>
  );
}
