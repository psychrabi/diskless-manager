import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/notification";
import { useSettings } from "@/hooks/useSettings";
import { Button, Card, Input } from "../ui";

import { useAppStore } from '@/store/useAppStore';

export default function HTTPConfigForm() {
  const { showNotification } = useNotification();
  const { updateHttp } = useSettings();
  const config = useAppStore(state => state.appConfig);

  const {
    register,
    handleSubmit,
    reset,
  } = useForm({
    defaultValues: {
      http_root: "/srv/tftp",
      http_server_ip: "*",
      http_server_port: "80"
    }
  });

  // Load saved config when config from store changes
  useEffect(() => {
    if (config?.settings?.http) {
      reset(config.settings.http);
    }
  }, [config, reset]);

  const onSubmit = async (data) => {
    showNotification(`Updating HTTP Configurations`, 'info');
    await updateHttp(data);
  };

  return (
    <Card title="HTTP Configuration" icon={Network} className=''>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <Input id="http_root" register={register("http_root")} label="HTTP Root Directory" placeholder="/srv/http" />
          <Input id="http_server_ip" register={register("http_server_ip")} label="HTTP Server IP" placeholder="*" />
          <Input id="http_server_port" register={register("http_server_port")} label="HTTP Server Port" placeholder="80" />
          <Button variant="primary" type="submit">Save HTTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}