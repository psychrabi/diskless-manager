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
