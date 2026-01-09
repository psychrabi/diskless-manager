import { useSettings } from "@/hooks/useSettings";
import { sambaSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import SambaForm from "./Forms/SambaForm";

export default function SambaConfigForm() {
  const { info } = useToastStore();
  const { updateSamba } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.samba || {},
    resolver: zodResolver(sambaSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating Samba Configurations`);
    await updateSamba(data);
  };

  return (
    <Card title="Samba Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <SambaForm
          register={register}
          errors={errors}
          config={config?.settings?.samba}
        />

        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save Samba Settings"}
        </Button>
      </form>
    </Card>
  );
}
