import { File } from 'lucide-react'
import { Button, Card } from '../ui'
import { useForm } from 'react-hook-form';

export default function BootFileConfigForm() {
  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm();

  return (
    <Card title="Boot File Configuration" icon={File} >
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
        <Button variant="primary">Save Boot Settings</Button>
      </div>
    </Card>
  )
}
