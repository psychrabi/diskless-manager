import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { Network } from "lucide-react";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";

export default function TFTPConfigForm() {
  const { info } = useToastStore();
  const { updateTftp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const { register, handleSubmit, reset } = useForm({
    defaultValues: {
      tftp_root: "/srv/tftp",
      tftp_server_ip: "0.0.0.0",
      tftp_options: "--secure",
    },
  });

  // Load saved config when config from store changes
  useEffect(() => {
    if (config?.settings?.tftp) {
      reset(config.settings.tftp);
    }
  }, [config, reset]);

  const onSubmit = async (data) => {
    info(`Updating TFTP Configurations`);
    await updateTftp(data);
  };

  return (
    <Card title="TFTP Configuration" icon={Network} className="">
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <Input
            id="tftp_root"
            register={register("tftp_root")}
            label="TFTP Root Directory"
            className="w-full"
            placeholder="/srv/tftp"
          />
          <Input
            id="tftp_server_ip"
            register={register("tftp_server_ip")}
            label="TFTP Server IP"
            className="w-full"
            placeholder="0.0.0.0"
          />
          <Input
            id="tftp_options"
            register={register("tftp_options")}
            label="TFTP Options"
            className="w-full"
            placeholder="--secure"
          />
          <Button variant="primary" type="submit">
            Save TFTP Settings
          </Button>
        </div>
      </form>
    </Card>
  );
}
