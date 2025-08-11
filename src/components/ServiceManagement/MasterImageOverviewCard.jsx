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
    <Card title="Master Image Overview" icon={HardDrive}>
      {loading ? (
        <div>Loading...</div>
      ) : overview ? (
        <div className="space-y-2">
          <div className="flex justify-between">
            <span className="font-semibold">Name:</span>
            <span>{overview.name}</span>
          </div>
          <div className="flex justify-between">
            <span className="font-semibold">Created:</span>
            <span>{overview.creation_date}</span>
          </div>
          <div className="flex justify-between">
            <span className="font-semibold">Clones:</span>
            <span>{overview.clones}</span>
          </div>
        </div>
      ) : (
        <div className="text-red-500">Failed to load master image overview.</div>
      )}
    </Card>
  );
};

export default MasterImageOverviewCard;
