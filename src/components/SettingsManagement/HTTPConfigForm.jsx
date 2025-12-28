import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { Network } from "lucide-react";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";

export default function HTTPConfigForm() {
  const { info } = useToastStore();
  const { updateHttp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const { register, handleSubmit, reset } = useForm({
    defaultValues: {
      root_dir: "/srv/tftp",
      server_ip: "*",
      port: "80",
    },
  });

  // Load saved config when config from store changes
  useEffect(() => {
    if (config?.settings?.http) {
      reset(config.settings.http);
    }
  }, [config, reset]);

  const onSubmit = async (data) => {
    info(`Updating HTTP Configurations`);
    await updateHttp(data);
  };

  return (
    <Card title="HTTP Configuration" icon={Network} className="">
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <Input
            id="root_dir"
            register={register("root_dir")}
            label="HTTP Root Directory"
            placeholder="/srv/http"
          />
          <Input
            id="server_ip"
            register={register("server_ip")}
            label="HTTP Server IP"
            placeholder="*"
          />
          <Input
            id="port"
            register={register("port")}
            label="HTTP Server Port"
            placeholder="80"
          />
          <Button variant="primary" type="submit">
            Save HTTP Settings
          </Button>
        </div>
      </form>
    </Card>
  );
}
