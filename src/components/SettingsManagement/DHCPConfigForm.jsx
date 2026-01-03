import { useSettings } from "@/hooks/useSettings";
import { dhcpSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Button, Card } from "../ui";
import DHCPForm from "./Forms/DHCPForm";

export default function DHCPConfigForm() {
  const { info } = useToastStore();
  const { updateDhcp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(dhcpSchema),
    defaultValues: config?.settings?.dhcp || {},
  });

  // Load saved config when config from store changes
  useEffect(() => {
    if (config?.settings?.dhcp) {
      reset(config.settings.dhcp);
    } else {
      reset({});
    }
  }, [config, reset]);

  const onSubmit = async (data) => {
    info(`Updating DHCP Configurations`);
    await updateDhcp(data);
  };

  return (
    <Card title="DHCP Server Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <DHCPForm
          register={register}
          errors={errors}
          config={config?.settings?.dhcp}
        />

        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save DHCP Settings"}
        </Button>
      </form>
    </Card>
  );
}
