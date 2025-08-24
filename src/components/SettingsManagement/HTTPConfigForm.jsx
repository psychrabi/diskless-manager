import { useForm, useFormContext } from "react-hook-form";
import { Button, Card, Input } from "../ui";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/NotificationContext";
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

export default function HTTPConfigForm() {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    defaultValues: {
      http_root: "/srv/tftp",
      http_server_ip: "*",
      http_server_port: "80"
    }
  });

  // Load saved config on component mount
  useEffect(() => {
    const loadConfig = async () => {
      try {
        const config = await invoke('read_config');
        if (config?.settings?.http) {
          reset(config.settings.http);
        }
      } catch (error) {
        console.error('Failed to load HTTP config:', error);
      }
    };
    loadConfig();
  }, [reset]);

  const onSubmit = async (data) => {
    console.log(data);
    showNotification(`Updating HTTP Configurations`, 'info');
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('configure_apache_server', { token, httpConfig: data })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      })
      .catch((error) => {
        showNotification(error, 'error');
        console.log(error);
      });
  };

  return (
    <Card title="HTTP Server Configuration" icon={Network} className=''>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <div className='grid grid-cols-2 gap-2'>
            <Input id="http_root" register={register("http_root")} label="HTTP Root Directory" className="w-full" placeholder="/srv/http"/>          
            <Input id="http_server_ip" register={register("http_server_ip")} label="HTTP Server IP" className="w-full" placeholder="*"/>            
            <Input id="http_server_port" register={register("http_server_port")} label="HTTP Server Port" className="w-full" placeholder="80"/>
          </div>
          <Button variant="primary" type="submit">Save HTTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}