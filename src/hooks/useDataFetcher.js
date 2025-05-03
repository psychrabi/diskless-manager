import { useState, useCallback, useEffect } from 'react';

const mockData = {
  clients: [
    { id: 'cl1', name: 'Client01', mac: '00:1A:2B:3C:4D:5E', ip: '192.168.1.101', clone: 'tank/client01-disk', target: 'iqn.2025-04.mydomain.server:client01', status: 'Online', isSuperClient: false },
    { id: 'cl2', name: 'Client02', mac: '00:1A:2B:3C:4D:5F', ip: '192.168.1.102', clone: 'tank/client02-disk', target: 'iqn.2025-04.mydomain.server:client02', status: 'Offline', isSuperClient: false },
    { id: 'cl3', name: 'Workstation-A', mac: 'AA:BB:CC:DD:EE:FF', ip: '192.168.1.103', clone: 'tank/workstation-a-disk', target: 'iqn.2025-04.mydomain.server:workstation-a', status: 'Online', isSuperClient: true },
  ],
  masters: [
    {
      id: 'm1',
      name: 'tank/win10-master',
      snapshots: [
        { id: 's1a', name: 'tank/win10-master@initial-install-v1', created: '2025-04-28 10:00:00', size: '15.2G', origin: null },
        { id: 's1b', name: 'tank/win10-master@final-apps-sysprepped-v1', created: '2025-05-01 14:30:00', size: '18.5G', origin: null },
      ]
    },
    {
      id: 'm2',
      name: 'tank/win11-dev-master',
      snapshots: [
        { id: 's2a', name: 'tank/win11-dev-master@base-vs2022', created: '2025-04-30 09:00:00', size: '25.1G', origin: null },
      ]
    },
  ],
  services: {
    iscsi: { name: 'iSCSI Target (target)', status: 'active' },
    dhcp: { name: 'DHCP Server (isc-dhcp-server)', status: 'active' },
    tftp: { name: 'TFTP Server (tftpd-hpa)', status: 'active' },
    zfs: { name: 'ZFS Kernel Module', status: 'active' },
  }
};

export const useDataFetcher = () => {
  const [data, setData] = useState({
    clients: [],
    masters: [],
    services: {}
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      // Simulate API call with mock data
      await new Promise(resolve => setTimeout(resolve, 500));
      setData(mockData);
    } catch (err) {
      console.error('Failed to fetch data:', err);
      setError('Failed to load data from server. Please check backend connection.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return {
    data,
    loading,
    error,
    fetchData,
    refresh: fetchData // Alias for better readability
  };
};
