import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/notification";
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

export default function HTTPConfigForm() {
  const { showNotification } = useNotification();

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
        showNotification('error', 'Failed to configure HTTP server', error.message || 'An unknown error occurred');
        console.log(error);
      });
  };

  return (
    <Card title="HTTP Configuration" icon={Network} className=''>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">          
            <Input id="http_root" register={register("http_root")} label="HTTP Root Directory" placeholder="/srv/http"/>          
            <Input id="http_server_ip" register={register("http_server_ip")} label="HTTP Server IP"  placeholder="*"/>            
            <Input id="http_server_port" register={register("http_server_port")} label="HTTP Server Port"  placeholder="80"/>          
          <Button variant="primary" type="submit">Save HTTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}