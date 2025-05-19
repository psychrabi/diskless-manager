import { useEffect, useState } from 'react';
import { Card } from '.';
import { invoke } from '@tauri-apps/api/core';

const ZfsPoolCard = ({ loading }) => {
  const [zpoolStats, setZpoolStats] = useState(null);

  useEffect(() => {
    // Replace with your actual invoke or fetch call
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
        <div className="space-y-2">
          <div><span className="font-semibold">Pool:</span> {zpoolStats.name}</div>
          <div><span className="font-semibold">Size:</span> {zpoolStats.size}</div>
          <div><span className="font-semibold">Used:</span> {zpoolStats.used}</div>
          <div><span className="font-semibold">Available:</span> {zpoolStats.available}</div>
        </div>
      ) : (
        <div className="text-red-500">Failed to load ZFS pool info.</div>
      )}
    </Card>
  )
};

export default ZfsPoolCard;