
import BootFileConfigForm from './BootFileConfigForm';
import DHCPConfigForm from './DHCPConfigForm';
import HTTPConfigForm from './HTTPConfigForm';
import TFTPConfigForm from './TFTPConfigForm';

const SettingManagement = ({ dhcp = {}, refresh }) => {

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold dark:text-gray-200">System Settings</h1>
      </div>
      <div className="grid gap-6 md:grid-cols-2">

        {/* DHCP Server Configuration */}
        <DHCPConfigForm />

        {/* Boot File Configuration */}
        <BootFileConfigForm />

        {/* TFTP Server Configuration */}
        <TFTPConfigForm />

        {/* TFTP Server Configuration */}
        <HTTPConfigForm />

        {/* Samba Server Configuration */}
        {/* <Card title="Samba Server Configuration" icon={Shield} >
          <div className="space-y-4">
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Enable Samba Sharing
              </label>
            </div>
            <div>
              <Input id="sambaShareName" type="text" defaultValue="diskless" placeholder="Share Name" />
            </div>
            <div>
              <Input id="sambaSharePath" type="text" defaultValue="/srv/tftp" placeholder="Share Path" />
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Read Only
              </label>
            </div>
            <div>
              <label className="label">
                <input type="checkbox" defaultChecked className="toggle" />
                Guest Access
              </label>
            </div>
            <Button variant="primary">Save Samba Settings</Button>
          </div>
        </Card> */}

        {/* Security Settings */}
        {/* <Card title="Security Settings" icon={Shield} >
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
              <Input id="sessionTimeout" type="number" defaultValue="30"  />
            </div>
            <Button variant="primary">Save Security Settings</Button>
          </div>
        </Card> */}

        {/* Notification Settings */}
        {/* <Card title="Notifications" icon={Bell} >

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
              <Input id="adminEmail" type="email" defaultValue="admin@bootserver.com"  />
            </div>
            <Button variant="primary">Save Notification Settings</Button>
          </div>
        </Card> */}
      </div>
      {/* System Information */}
      {/* <Card title="System Information" icon={Bell} >
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
      </Card> */}
    </div>
  );
}

export default SettingManagement;