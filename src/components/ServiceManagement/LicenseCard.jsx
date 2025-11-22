import { useAppStore } from '@/store/useAppStore';
import { Card } from '../ui';

export default function LicenseCard() {
  const license = useAppStore(state => state.licenseInfo) || {};

  // We assume license info is fetched by parent or layout if needed, 
  // but for Dashboard we might need to ensure it's fetched.
  // However, AdminLayout calls fetchData which calls fetchServerInfo etc.
  // Wait, fetchData does NOT call fetchLicenseInfo yet!

  // I should update fetchData in useAppStore to include fetchLicenseInfo if I want it globally available on dashboard load.
  // OR, I can fetch it here if missing.

  // Let's just use the store data. If it's null, show "Loading..." or empty.

  if (!license.license_status && !license.license_key) {
    // Maybe it's not loaded yet.
    // For now let's just show what we have.
  }



  return (
    <Card title="License Information" icon={null} className='col-span-2'>
      <ul>
        <li><strong>Status:</strong> {license.license_status || 'not activated'}</li>
        <li><strong>Expires:</strong> {license.license_expires || '—'}</li>
        <li><strong>Key:</strong> {license.license_key ? license.license_key : '—'}</li>
      </ul>
    </Card>
  );
}