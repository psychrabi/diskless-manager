import { invoke } from '@tauri-apps/api/core';
import { RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button, Card } from '../components/ui';
import { useNotification } from '../contexts/NotificationContext';

export const RAMUsage = () => {
  const [ramUsage, setRamUsage] = useState(null);
  const [arcStat, setArcStat] = useState(null);
  const [loading, setLoading] = useState(true);
  const { showNotification } = useNotification();

  const fetchRamUsage = async () => {
    await invoke('get_ram_usage').then((response) => {
      setRamUsage(response);
      setLoading(false);
      if (response.message) showNotification(response.message, 'success');
    }).catch((err) => showNotification(err, 'error'));
  };

  const fetchArcStat = async () => {
    await invoke('get_zfs_arcstat').then((response) => {
      setArcStat(response);
    }).catch(() => setArcStat(null));
  };

  const clearRamCache = async () => {
    await invoke('clear_ram_cache').then((response) => {
      if (response.message) showNotification(response.message, 'success');
    }).catch((err) => showNotification(err, 'error'));
  };

  useEffect(() => {
    fetchRamUsage();
    fetchArcStat();
    // Refresh every 5 minutes
    const interval = setInterval(() => {
      fetchRamUsage();
      fetchArcStat();
    }, 300000);
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return (
      <Card title="RAM Usage" icon={RefreshCw}>
        <div className="text-center py-4 text-gray-500">Loading RAM usage...</div>
      </Card>
    );
  }

  return (
    <Card title="RAM Usage" icon={RefreshCw} actions={<Button onClick={clearRamCache} variant="primary" className="w-full btn-xs">Clear Cache</Button>}>
      <ul className="">
        <div className="grid grid-cols-2 gap-x-10 space-y-2">
          <li className='flex justify-between'><span className="font-semibold">Total:</span> {ramUsage.memory.total}</li>
          <li className='flex justify-between'><span className="font-semibold">Used:</span> {ramUsage.memory.used}</li>
          <li className='flex justify-between'><span className="font-semibold">Free:</span> {ramUsage.memory.free}</li>
          <li className='flex justify-between'><span className="font-semibold">Available:</span> {ramUsage.memory.available}</li>
          {arcStat && (<>
            <li className='flex justify-between'><span className="font-semibold">ZFS Cache:</span> {(arcStat.size / (1024 * 1024)).toFixed(1)} MB</li>
            <li className='flex justify-between'><span className="font-semibold">ZFS Cache Hit:</span> {arcStat.hit_percent.toFixed(2)}%</li>
          </>
          )}
        </div>
      </ul>
    </Card>
  );
};
