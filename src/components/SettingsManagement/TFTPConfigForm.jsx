import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/notification";
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

export default function TFTPConfigForm() {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    reset,
  } = useForm({
    defaultValues: {
      tftp_root: "/srv/tftp",
      tftp_server_ip: "0.0.0.0",
      tftp_options: "--secure"
    }
  });

  // Load saved config on component mount
  useEffect(() => {
    const loadConfig = async () => {
      try {
        const config = await invoke('read_config');
        if (config?.settings?.tftp) {
          reset(config.settings.tftp);
        }
      } catch (error) {
        console.error('Failed to load TFTP config:', error);
      }
    };
    loadConfig();
  }, [reset]);

  const onSubmit = async (data) => {
    console.log(data);
    showNotification(`Updating TFTP Configurations`, 'info');
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('configure_tftp_server', { token, tftpConfig: data })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      })
      .catch((error) => {
        showNotification(error, 'error');
        console.log(error);
      });
  };

  return (
    <Card title="TFTP Configuration" icon={Network} className=''>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">          
            <Input id="tftp_root" register={register("tftp_root")} label="TFTP Root Directory" className="w-full" placeholder="/srv/tftp" />
            <Input id="tftp_server_ip" register={register("tftp_server_ip")} label="TFTP Server IP" className="w-full" placeholder="0.0.0.0" />
            <Input id="tftp_options" register={register("tftp_options")} label="TFTP Options" className="w-full" placeholder="--secure" />          
          <Button variant="primary" type="submit">Save TFTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}