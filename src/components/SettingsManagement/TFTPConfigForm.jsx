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
      root_dir: "/srv/tftp",
      server_ip: "0.0.0.0",
      port: "69",
      options: "--secure",
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
            id="root_dir"
            register={register("root_dir")}
            label="TFTP Root Directory"
            className="w-full"
            placeholder="/srv/tftp"
          />
          <Input
            id="server_ip"
            register={register("server_ip")}
            label="TFTP Server IP"
            className="w-full"
            placeholder="0.0.0.0"
          />
          <Input
            id="port"
            register={register("port")}
            label="TFTP Server Port"
            className="w-full"
            placeholder="69"
          />
          <Input

            id="options"
            register={register("options")}
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
