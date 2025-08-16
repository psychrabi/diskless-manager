import { File } from 'lucide-react'
import { Button, Card } from '../ui'
import { useForm } from 'react-hook-form';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';

export default function BootFileConfigForm() {
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
    showNotification(`Updating Boot File Configurations`, 'info');
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    // For now, we'll just save the TFTP configuration since that's what's implemented
    // In the future, we might want to save boot files to a specific directory
    await invoke('configure_tftp_server', { token, tftp_root: '/srv/tftp' })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      })
      .catch((error) => {
        showNotification(error, 'error');
        console.log(error);
      });
  };

  return (
    <Card title="Boot File Configuration" icon={File} >
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className='space-y-4'>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>Legacy Boot file</label>
            <input className="input w-full" id="boot_file_legacy" {...register('boot_file_legacy')} placeholder="Eg. ipxe.pxe, ipxe.kpxe" />
          </fieldset>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>UEFI32 Boot file</label>
            <input className="input  w-full" id="boot_file_uefi32" {...register('boot_file_uefi32')} placeholder="Eg. ipxe32.efi" />
          </fieldset>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>UEFI64 Boot file</label>
            <input className="input w-full" id="boot_file_uefi64" {...register('boot_file_uefi64')} placeholder="Eg. ipxe.efi, snponly.efi" />
          </fieldset>
          <Button variant="primary" type="submit">Save Boot Settings</Button>
        </div>
      </form>
    </Card>
  )
}
