import { useForm } from "react-hook-form";
import { Button, Card } from "../ui";
import { Network } from "lucide-react";
import { useNotification } from "@/contexts/NotificationContext";
import { invoke } from "@tauri-apps/api/core";


export default function TFTPConfigForm() {
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
    showNotification(`Updating TFTP Configurations`, 'info');
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('configure_tftp_server', { token, tftp_root: data.tftpDirectory })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      })
      .catch((error) => {
        showNotification(error, 'error');
        console.log(error);
      });
  };

  return (
    <Card title="TFTP Server Configuration" icon={Network} className=''>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <div className='flex gap-2'>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>TFTP Server IP</label>
              <input className="input w-full" id="tftpServer" defaultValue="192.168.1.50" {...register("tftpServer")} />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>TFTP Root Directory</label>
              <input className="input w-full" id="tftpDirectory" defaultValue="/srv/tftp" {...register("tftpDirectory")} />
            </fieldset>
          </div>
          <div className='flex gap-2'>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>TFTP Address</label>
              <input className="input w-full" id="tftpAddress" defaultValue="192.168.1.50:69" {...register("tftpAddress")} />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>TFTP Options</label>
              <input className="input w-full" id="tftpOptions" defaultValue="--secure" {...register("tftpOptions")} />
            </fieldset>
          </div>
          <Button variant="primary" type="submit">Save TFTP Settings</Button>
        </div>
      </form>
    </Card>
  )
}