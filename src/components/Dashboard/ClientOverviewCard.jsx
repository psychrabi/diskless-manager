import { invoke } from '@tauri-apps/api/core';
import { Users } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';
import { useNotification } from '@/contexts/notification';

const ClientOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);
  const { showNotification } = useNotification();

  useEffect(() => {
    const fetchClientOverview = async () => {
      try {
        const data = await invoke('get_client_overview');
        setOverview(data);
      } catch (err) {
        showNotification('error', 'Failed to load client overview', err.message || 'An unknown error occurred');
        console.error(err);
        setOverview(null);
      } finally {
        setLoading(false);
      }
    };
    fetchClientOverview();
  }, [showNotification]);

  return (
    <Card title="Client Overview" icon={Users}>
      {loading ? (
        <div>Loading...</div>
      ) : overview ? (
        <ul className="space-y-2 ">
          <li className="flex justify-between">
            <span className="font-semibold">Total Clients:</span>
            {overview.total_clients}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Active Clients:</span>
            {overview.active_clients}
          </li>
        </ul>
      ) : (
        <div className="text-red-500">Failed to load client overview.</div>
      )}
    </Card>
  );
};

export default ClientOverviewCard;
