import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import { getServiceIcon } from "@/constants/serviceIcons";

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
    control,
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
    <Card title={`${title} Configuration`} icon={getServiceIcon(section)} className="h-full xl:col-span-2">
      <form onSubmit={handleSubmit(onSubmit)}>
        <FormComponent
          control={control}
          register={register}
          errors={errors}
          config={config?.settings?.[section]}
        />
        <div className="flex justify-end mt-4">
          <Button
            variant="primary"
            type="submit"
            loading={isSubmitting}
          >
            {isSubmitting ? "Saving\u2026" : `Save ${sectionKey} Settings`}
          </Button>
        </div>
      </form>
    </Card>
  );
};

export default ConfigForm;
