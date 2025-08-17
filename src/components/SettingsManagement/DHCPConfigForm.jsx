import z from 'zod';
import { Button, Card } from '../ui';
import { zodResolver } from '@hookform/resolvers/zod';
import { useNotification } from '@/contexts/NotificationContext';
import { useForm } from 'react-hook-form';
import { HardDrive, Network } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useEffect } from 'react';

const dhcpSchema = z.object({
  subnet_ip: z.ipv4(),
  start_ip: z.ipv4(),
  end_ip: z.ipv4(),
  subnet_mask: z.ipv4(),
  gateway_ip: z.ipv4(),
  dns_server1: z.ipv4(),
  dns_server2: z.ipv4(),
  broadcast_ip: z.ipv4(),
  boot_server_ip: z.ipv4(),
  boot_file_legacy: z.string().optional(),
  boot_file_uefi32: z.string().optional(),
  boot_file_uefi64: z.string().optional(),
});

const dhcpInitial = {
  subnet_ip: "192.168.1.0",
  start_ip: "192.168.1.120",
  end_ip: "192.168.1.130",
  subnet_mask: "255.255.255.0",
  gateway_ip: "192.168.1.254",
  dns_server1: "1.1.1.1",
  dns_server2: "1.0.0.1",
  broadcast_ip: "192.168.1.255",
  boot_server_ip: "192.168.1.250",
  boot_file_legacy: "ipxe.kpxe",
  boot_file_uefi32: "ipxe.efi",
  boot_file_uefi64: "ipxe.efi",
}

export default function DHCPConfigForm() {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(dhcpSchema),
    defaultValues: dhcpInitial
  });

  // Load saved config on component mount
  useEffect(() => {
    const loadConfig = async () => {
      try {
        const config = await invoke('read_config');
        if (config?.settings?.dhcp) {
          reset(config.settings.dhcp);
        } else {
          reset(dhcpInitial);
        }
      } catch (error) {
        console.error('Failed to load DHCP config:', error);
        reset(dhcpInitial);
      }
    };
    loadConfig();
  }, [reset]);

  const onSubmit = async (data) => {
    console.log(data);
    showNotification(`Updating DHCP Configurations`, 'info');
    const token = localStorage.getItem('authToken') || '';
    
    try {
      await invoke('configure_dhcp_server', { token, config: data });
      showNotification('DHCP configuration saved successfully', 'success');
    } catch (error) {
      showNotification(error, 'error');
      console.error(error);
    }
  };

  return (
    <Card title="DHCP Server Configuration" icon={Network} >
      {Object.keys(errors).length > 0 && (
        <div className="mb-4 text-red-500 text-sm">
          {Object.entries(errors).map(([field, error]) => (
            <div key={field}>{error.message}</div>
          ))}
        </div>
      )}
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <div className='flex gap-2 '>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>DHCP Start IP</label>
              <input className="input w-full" id="dhcpstart_ip" {...register('start_ip')} placeholder="192.168.1.100" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>DHCP End IP</label>
              <input className="input w-full" id="dhcpend_ip" {...register('end_ip')} placeholder="192.168.1.200" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>Subnet Mask</label>
              <input className="input w-full" id="subnet_mask" {...register('subnet_mask')} placeholder="255.255.255.0" />
            </fieldset>
          </div>
          <div className='flex gap-2'>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>Gateway IP</label>
              <input className="input w-full" id="gateway_ip" {...register('gateway_ip')} placeholder="192.168.1.1" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>DNS Server 1</label>
              <input className="input w-full" id="dns_server1" {...register('dns_server1')} placeholder="1.1.1.1" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>DNS Server 2</label>
              <input className="input w-full" id="dns_server2" {...register('dns_server2')} placeholder="1.0.0.1" />
            </fieldset>
          </div>
          <div className='flex gap-2'>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>Subnet IP</label>
              <input className="input w-full" id="subnet_ip" {...register('subnet_ip')} placeholder="192.168.1.0" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>Boot Server</label>
              <input className="input w-full" id="boot_server_ip" {...register('boot_server_ip')} placeholder="192.168.1.1" />
            </fieldset>
            <fieldset className='fieldset flex-1'>
              <label className='fieldset-legend'>Broadcast IP</label>
              <input className="input w-full" id="broadcast_ip" {...register('broadcast_ip')} placeholder="192.168.1.1" />
            </fieldset>
          </div>
          <Button variant="primary" type="submit">Save Server Settings</Button>
        </div>
      </form>
    </Card>
  )
}
