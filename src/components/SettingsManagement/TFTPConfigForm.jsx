import { useForm } from "react-hook-form";
import { Button, Card } from "../ui";
import { Network } from "lucide-react";




export default function TFTPConfigForm() {
  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm();

  return (
    <Card title="TFTP Server Configuration" icon={Network} className=''>
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
        <Button variant="primary">Save TFTP Settings</Button>
      </div>
    </Card>
  )
}