import { invoke } from '@tauri-apps/api/core';
import { HardDrive } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';

const MasterImageOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke('get_master_image_overview')
      .then(data => {
        setOverview(data);
        setLoading(false);
      })
      .catch(err => {
        console.error(err);
        setLoading(false);
      });
  }, []);

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
