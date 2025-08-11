import { invoke } from '@tauri-apps/api/core';
import { Users } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';

const ClientOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke('get_client_overview')
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
    <Card title="Client Overview" icon={Users}>
      {loading ? (
        <div>Loading...</div>
      ) : overview ? (
        <div className="space-y-2">
          <div className="flex justify-between">
            <span className="font-semibold">Total Clients:</span>
            <span>{overview.total_clients}</span>
          </div>
          <div className="flex justify-between">
            <span className="font-semibold">Active Clients:</span>
            <span>{overview.active_clients}</span>
          </div>
        </div>
      ) : (
        <div className="text-red-500">Failed to load client overview.</div>
      )}
    </Card>
  );
};

export default ClientOverviewCard;
