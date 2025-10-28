import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { Card } from '../ui';
import { useNotification } from '@/contexts/notification';

const ZfsPoolCard = ({ loading }) => {
  const [zpoolStats, setZpoolStats] = useState(null);
  const { showNotification } = useNotification();

  useEffect(() => {
    const fetchZpoolStats = async () => {
      try {
        const stats = await invoke("get_zpool_list");
        setZpoolStats(stats[0]);
      } catch (error) {
        showNotification('error', 'Failed to load ZFS pool info', error.message || 'An unknown error occurred');
        setZpoolStats(null);
      }
    };
    fetchZpoolStats();
  }, [showNotification]);
  return (
    <Card title="ZFS Pool Usage">
      {loading ? (
        <div>Loading...</div>
      ) : zpoolStats ? (
        <ul className="">
          <div className='grid grid-cols-2 gap-x-10 space-y-2'>
            <li className='flex justify-between'>
              <span className="font-semibold">Pool:</span> {zpoolStats.name}
            </li>
            <li className='flex justify-between'>
              <span className="font-semibold">Size:</span> {zpoolStats.size}
            </li>
            <li className='flex justify-between'>
              <span className="font-semibold">Used:</span> {zpoolStats.alloc}</li>
            <li className='flex justify-between'>
              <span className="font-semibold">Available:</span> {zpoolStats.free}
            </li>
            <li className='flex justify-between'>
              <span className="font-semibold">Health:</span> <span className='badge badge-success rounded-full'>{zpoolStats.health}</span>
            </li>
          </div>
        </ul>
      ) : (
        <div className="text-red-500">Failed to load ZFS pool info.</div>
      )}
    </Card>
  )
};

export default ZfsPoolCard;