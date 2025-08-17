import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/NotificationContext";
import { invoke } from "@tauri-apps/api/core";


export default function HTTPConfigForm() {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm();

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
            <Input id="http_root" defaultValue="/srv/http" register={register("http_root")} label="HTTP Root Directory" className="w-full" placeholder="/srv/http"/>          
            <Input id="http_server_ip" defaultValue="192.168.1.50" register={register("http_server_ip")} label="HTTP Server IP" className="w-full" placeholder="192.168.1.250"/>            
            <Input id="http_server_port" defaultValue="80" register={register("http_server_port")} label="HTTP Server Port" className="w-full" placeholder="80"/>
          </div>
          <Button variant="primary" type="submit">Save HTTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}