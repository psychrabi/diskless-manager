
import { GitPullRequestArrow, Settings } from 'lucide-react';
import { Card } from '../ui';
import AdminPassword from '../ApplicationMangement/AdminPasswordForm';
import DHCPConfigForm from './DHCPConfigForm';
import HTTPConfigForm from './HTTPConfigForm';
import TFTPConfigForm from './TFTPConfigForm';
import LicenseActivation from '../LicenseManagement/LicenseActivation';

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



          {/* Boot File Configuration */}
          <AdminPassword />

          {/* TFTP Server Configuration */}
          <TFTPConfigForm />

          {/* TFTP Server Configuration */}
          <HTTPConfigForm />
        </div>
        {/* Information Panel */}


        <Card title="Boot Process Overview" icon={GitPullRequestArrow} className='bg-base-100'>
          <ul className="steps steps-vertical lg:steps-horizontal w-full">
            <li className="step step-primary">
              <div className="flex flex-col items-center mt-2">
                <span className="font-bold">DHCP</span>
                <span
                  className="text-xs text-base-content/60 text-center max-w-[150px]"
                >Client requests IP and boot server info</span
                >
              </div>
            </li>
            <li className="step step-primary">
              <div className="flex flex-col items-center mt-2">
                <span className="font-bold">TFTP</span>
                <span
                  className="text-xs text-base-content/60 text-center max-w-[150px]"
                >Client downloads bootloader and kernel</span
                >
              </div>
            </li>
            <li className="step step-primary">
              <div className="flex flex-col items-center mt-2">
                <span className="font-bold">iSCSI</span>
                <span
                  className="text-xs text-base-content/60 text-center max-w-[150px]"
                >Client connects to disk image</span
                >
              </div>
            </li>
            <li className="step step-primary">
              <div className="flex flex-col items-center mt-2">
                <span className="font-bold">Boot</span>
                <span
                  className="text-xs text-base-content/60 text-center max-w-[150px]"
                >OS boots from network storage</span
                >
              </div>
            </li>
          </ul>
        </Card>
      </div>
    </Card>
  );
}

export default SettingsManagement;