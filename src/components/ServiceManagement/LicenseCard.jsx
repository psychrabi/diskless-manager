import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card } from '../ui';

export default function LicenseCard() {
  const [license, setLicense] = useState({
    license_key: null,
    license_status: null,
    license_expires: null,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    async function fetchLicense() {
      try {
        const info = await invoke('get_license_info');
        setLicense(info || {});
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    }
    fetchLicense();
  }, []);

  if (loading) return <div>Loading license...</div>;
  if (error) return <div className="text-error">Error: {error}</div>;

  return (
    <Card title="License Information" icon={null}  className='col-span-2'>
      <ul>
        <li><strong>Status:</strong> {license.license_status || 'not activated'}</li>
        <li><strong>Expires:</strong> {license.license_expires || '—'}</li>
        <li><strong>Key:</strong> {license.license_key ? license.license_key : '—'}</li>
      </ul>
    </Card>
  );
}