import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { Card } from '../ui';

const ZfsPoolCard = ({ loading }) => {
  const [zpoolStats, setZpoolStats] = useState(null);

  useEffect(() => {
    invoke("get_zpool_stats")
      .then((stats) => {
        setZpoolStats(stats);
      })
  }, []);
  return (
    <Card title="ZFS Pool Usage">
      {loading ? (
        <div>Loading...</div>
      ) : zpoolStats ? (
        <div className="space-y-2 flex">
          <div className='flex-auto'>
            <span className="font-semibold">Pool:</span> {zpoolStats.name} <br />
            <span className="font-semibold">Size:</span> {zpoolStats.size}
          </div>
          <div className='flex-auto'><span className="font-semibold">Used:</span> {zpoolStats.used}<br />
            <span className="font-semibold">Available:</span> {zpoolStats.available}
          </div>
        </div>
      ) : (
        <div className="text-red-500">Failed to load ZFS pool info.</div>
      )}
    </Card>
  )
};

export default ZfsPoolCard;