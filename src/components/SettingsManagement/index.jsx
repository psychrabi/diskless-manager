
import { Button, Card, Input } from '@/components/ui';
import { useNotification } from '@/contexts/NotificationContext';
import { zodResolver } from '@hookform/resolvers/zod';
import { invoke } from '@tauri-apps/api/core';
import { Bell, HardDrive, Network, Shield } from 'lucide-react';
import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import z from 'zod/v4';
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


const SettingManagement = ({ dhcp = {}, refresh }) => {

  const { showNotification } = useNotification();
  const dhcpInitial = {
    subnet_ip: dhcp.subnet_ip ?? "192.168.1.0",
    start_ip: dhcp?.start_ip ?? "192.168.1.120",
    end_ip: dhcp.end_ip ?? "192.168.1.130",
    subnet_mask: dhcp.subnet_mask ?? "255.255.255.0",
    gateway_ip: dhcp.gateway_ip ?? "192.168.1.254",
    dns_server1: dhcp.dns_server1 ?? "1.1.1.1",
    dns_server2: dhcp.dns_server2 ?? "1.0.0.1",
    broadcast_ip: dhcp.broadcast_ip ?? "192.168.1.255",
    boot_server_ip: dhcp.boot_server_ip ?? "192.168.1.250",
    boot_file_legacy: dhcp.boot_file_legacy ?? "ipxe.kpxe",
    boot_file_uefi32: dhcp.boot_file_uefi32 ?? "ipxe.efi",
    boot_file_uefi64: dhcp.boot_file_user64 ?? "ipxe.efi",
  }
  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm({
    resolver: zodResolver(dhcpSchema),
    defaultValues:  dhcpInitial,
  });

  // Keep form in sync with client prop
  // useEffect(() => {
  //   reset(dhcpInitial);
  // }, [dhcp, reset]);

  const onSubmit = async (data) => {
    console.log(data)
    showNotification(`Updating DHCP Configurations`, 'info');
    await invoke('configure_dhcp_server', { config: data }).then((response) => {
      if (response.message) showNotification(response.message, 'success');
    }).catch((error) => {
      showNotification(error, 'error');
      console.log(error)
    })

  };


  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold dark:text-gray-200">System Settings</h1>
      </div>


      <div className="grid gap-6 md:grid-cols-2">

        {/* Server Configuration */}
        <Card title="Server Configuration" icon={HardDrive} >
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
                <input className="flex-1" id="dhcpstart_ip" {...register('start_ip')} placeholder="192.168.1.100" label="DHCP Start IP" />
                <input className="flex-1" id="dhcpend_ip" {...register('end_ip')} placeholder="192.168.1.200" label="DHCP End IP" />
                <input className="flex-1" id="subnet_mask" {...register('subnet_mask')} placeholder="255.255.255.0" label="Subnet Mask" />
              </div>
              <div className='flex gap-2'>
                <input className="flex-1" id="gateway_ip" {...register('gateway_ip')} placeholder="192.168.1.1" label={"Gateway IP"} />
                <input className="flex-1" id="dns_server1" {...register('dns_server1')} placeholder="1.1.1.1" label={"DNS Server 1"} />
                <input className="flex-1" id="dns_server2" {...register('dns_server2')} placeholder="1.0.0.1" label={"DNS Server 2"} />
              </div>
              <div className='flex gap-2'>
                <input className="flex-1" id="subnet_ip" {...register('subnet_ip')} placeholder="192.168.1.0" label={"Subnet IP"} />
                <input className="flex-1" id="boot_server_ip" {...register('boot_server_ip')} placeholder="192.168.1.1" label={"Boot Server"} />
                <input className="flex-1" id="broadcast_ip" {...register('broadcast_ip')} placeholder="192.168.1.1" label={"Broadcast IP"} />

              </div>
              <div className='flex gap-2'>
                <input className="flex-1" id="boot_file_legacy" {...register('boot_file_legacy')} placeholder="Eg. ipxe.pxe, ipxe.kpxe" label={"Legacy Boot file"} />

                <input className="flex-1" id="boot_file_uefi32" {...register('boot_file_uefi32')} placeholder="Eg. ipxe32.efi" label={"UEFI32 Boot file"} />
                <input className="flex-1" id="boot_file_uefi64" {...register('boot_file_uefi64')} placeholder="Eg. ipxe.efi, snponly.efi" label={"UEFI64 Boot file"} />
              </div>
              <Button variant="primary" type="submit">Save Server Settings</Button>
            </div>
          </form>

        </Card>

        {/* Network Configuration */}
        <Card title="Network Configuration" icon={Network} >
          <div className="space-y-4">
            <div>
              <Input id="tftpServer" defaultValue="192.168.1.50" label="TFTP Server IP" />
            </div>
            <div>
              <Input id="bootFile" defaultValue="/pxelinux.0" label="Boot File Path" />
            </div>
            <div>
              <Input id="nfsServer" defaultValue="192.168.1.50" label="NFS Server" />
            </div>
            <div>
              <Input id="nfsPath" defaultValue="/srv/nfs/diskless" label="NFS Export Path" />
            </div>
            <Button variant="primary">Save Network Settings</Button>
          </div>
        </Card>

        {/* Security Settings */}
        <Card title="Security Settings" icon={Shield} >
          <div className="space-y-4">
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Enable Authentication
              </label>
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Enable SSL/TLS
              </label>
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Enable Firewall
              </label>
            </div>
            <div>
              <Input id="sessionTimeout" type="number" defaultValue="30" label="Session Timeout (minutes)" />
            </div>
            <Button variant="primary">Save Security Settings</Button>
          </div>
        </Card>

        {/* Notification Settings */}
        <Card title="Notifications" icon={Bell} >

          <div className="space-y-4">
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Email Notifications
              </label>
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Security Alerts
              </label>
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                User Activity Alerts
              </label>
            </div>
            <div>
              <Input id="adminEmail" type="email" defaultValue="admin@bootserver.com" label="Admin Email" />
            </div>
            <Button variant="primary">Save Notification Settings</Button>
          </div>
        </Card>
      </div>
      {/* System Information */}
      <Card title="System Information" icon={Bell} >
        <div>
          <div className="grid gap-4 md:grid-cols-3">
            <div>
              <h4 className="font-medium">Server Version</h4>
              <p className="text-sm text-muted-foreground">v2.4.1</p>
            </div>
            <div>
              <h4 className="font-medium">Uptime</h4>
              <p className="text-sm text-muted-foreground">15 days, 8 hours</p>
            </div>
            <div>
              <h4 className="font-medium">Last Backup</h4>
              <p className="text-sm text-muted-foreground">2024-07-26 03:00 AM</p>
            </div>
            <div>
              <h4 className="font-medium">Database Size</h4>
              <p className="text-sm text-muted-foreground">125 MB</p>
            </div>
            <div>
              <h4 className="font-medium">Log Files</h4>
              <p className="text-sm text-muted-foreground">2.3 GB</p>
            </div>
            <div>
              <h4 className="font-medium">Available Space</h4>
              <p className="text-sm text-muted-foreground">89.5 GB</p>
            </div>
          </div>
          <div className="divider"></div>
          <div className="flex gap-2">
            <Button variant="primary">Export Logs</Button>
            <Button variant="accent">Backup Database</Button>
            <Button variant="info">System Diagnostics</Button>
          </div>
        </div>
      </Card>
    </div>
  );
}

export default SettingManagement;