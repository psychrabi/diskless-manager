import React, { useState, useEffect } from 'react';
import { Card, Button } from '../components/ui';
import { RefreshCw, Database } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
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
    <Card title="RAM Usage" icon={RefreshCw} actions={<Button onClick={clearRamCache} variant="primary" className="w-full">Clear Cache</Button>}>
      <div className="space-y-2">
        <div className="grid grid-cols-2">
          <div>Total: {ramUsage.memory.total}</div>
          <div>Used: {ramUsage.memory.used}</div>
          <div>Free: {ramUsage.memory.free}</div>
          <div>Available: {ramUsage.memory.available}</div>
          {arcStat && (<>
            <div>ZFS Cache: {(arcStat.size / (1024 * 1024)).toFixed(1)} MB</div>
            <div>ZFS Cache Hit: {arcStat.hit_percent.toFixed(2)}%</div>
          </>
          )}
        </div>
      </div>
    </Card>
  );
};
