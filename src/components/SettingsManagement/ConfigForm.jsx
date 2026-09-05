import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";

const UPDATE_MAP = {
  client_lifecycle: "updateClientLifecycle",
  dhcp: "updateDhcp",
  tftp: "updateTftp",
  http: "updateHttp",
  samba: "updateSamba",
  iscsi: "updateIscsi",
};

const ConfigForm = ({ schema, section, title, FormComponent }) => {
  const { info } = useToastStore();
  const settings = useSettings();
  const config = useAppStore((state) => state.appConfig);
  const updateFn = settings[UPDATE_MAP[section]];

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(schema),
    defaultValues: config?.settings?.[section] || (section === "client_lifecycle" ? { non_persistent_reset_delay_minutes: 5 } : {}),
  });

  useEffect(() => {
    if (config?.settings?.[section]) {
      reset(config.settings[section]);
    } else {
      reset(section === "client_lifecycle" ? { non_persistent_reset_delay_minutes: 5 } : {});
    }
  }, [config, section, reset]);

  const onSubmit = async (data) => {
    info(`Updating ${title} Configurations`);
    await updateFn(data);
  };

  const sectionKey = section === "client_lifecycle" ? "Client Reset" : section.charAt(0).toUpperCase() + section.slice(1);

  return (
    <Card title={`${title} Configuration`} icon={Network} className="xl:col-span-2">
      <form onSubmit={handleSubmit(onSubmit)}>
        <FormComponent
          register={register}
          errors={errors}
          config={config?.settings?.[section]}
        />
        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving\u2026" : `Save ${sectionKey} Settings`}
        </Button>
      </form>
    </Card>
  );
};

export default ConfigForm;
