
import { Button, Card, Input } from '@/components/ui';
import { Bell, HardDrive, Network, Shield } from 'lucide-react';

const SettingManagement = () => {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold dark:text-gray-200">System Settings</h1>
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

      <div className="grid gap-6 md:grid-cols-2">

        {/* Server Configuration */}
        <Card title="Server Configuration" icon={HardDrive} >
          <div className="space-y-4">
            <div>
              <Input id="dhcpRange" defaultValue="192.168.1.100-192.168.1.200" label="DHCP Range" />
            </div>
            <div>
              <Input id="subnetMask" defaultValue="255.255.255.0" label="Subnet Mask" />
            </div>
            <div>
              <Input id="gatewayIp" defaultValue="192.168.1.1" label={"Gateway IP"} />
            </div>
            <div>
              <Input id="dnsServer" defaultValue="8.8.8.8" label={"DNS Server"} />
            </div>
            <Button variant="primary">Save Server Settings</Button>
          </div>
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
    </div>
  );
}

export default SettingManagement;