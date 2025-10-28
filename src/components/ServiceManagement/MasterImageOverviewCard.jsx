import { invoke } from '@tauri-apps/api/core';
import { HardDrive } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';
import { useNotification } from '@/contexts/notification';

const MasterImageOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);
  const { showNotification } = useNotification();

  useEffect(() => {
    const fetchMasterImageOverview = async () => {
      try {
        const data = await invoke('get_default_image_overview');
        setOverview(data);
      } catch (err) {
        showNotification('error', 'Failed to load master image overview', err.message || 'An unknown error occurred');
        console.error(err);
        setOverview(null);
      } finally {
        setLoading(false);
      }
    };
    fetchMasterImageOverview();
  }, [showNotification]);

  return (
    <Card title="Default Image Overview" icon={HardDrive}>
      {loading ? (
        <div>Loading...</div>
      ) : overview ? (
        <ul className="space-y-2">
          <li className="flex justify-between">
            <span className="font-semibold">Name:</span>
            {overview.name}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Created:</span>
            {overview.creation_date}
          </li>
        </ul>
      ) : (
        <div className="text-red-500">Set a default image first.</div>
      )}
    </Card>
  );
};

export default MasterImageOverviewCard;
