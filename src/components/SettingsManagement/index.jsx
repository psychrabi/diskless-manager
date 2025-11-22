
import { GitPullRequestArrow, Settings } from 'lucide-react';
import { Card } from '../ui';
import AdminPassword from './AdminPasswordForm';
import DHCPConfigForm from './DHCPConfigForm';
import HTTPConfigForm from './HTTPConfigForm';
import TFTPConfigForm from './TFTPConfigForm';
import LicenseActivation from './LicenseActivation';

import { useEffect } from 'react';
import { useAppStore } from '@/store/useAppStore';

const SettingsManagement = () => {
  const fetchConfig = useAppStore(state => state.fetchConfig);
  const fetchLicenseInfo = useAppStore(state => state.fetchLicenseInfo);

  useEffect(() => {
    fetchConfig();
    fetchLicenseInfo();
  }, [fetchConfig, fetchLicenseInfo]);

  return (
    <Card title="System Settings" icon={Settings} className='bg-base-300'>
      <div className="min-h-[calc(100vh-14rem)] space-y-6">
        <DHCPConfigForm />
        <div className="grid gap-6 md:grid-cols-2">

          {/* License activation */}
          <LicenseActivation />

          {/* Boot File Configuration */}
          <AdminPassword />

          {/* TFTP Server Configuration */}
          <TFTPConfigForm />

          {/* TFTP Server Configuration */}
          <HTTPConfigForm />
        </div>
      </div>
    </Card>
  );
}

export default SettingsManagement;