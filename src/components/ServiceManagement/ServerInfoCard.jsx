import { invoke } from '@tauri-apps/api/core';
import { List, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';
import { useNotification } from '@/contexts/notification';

const ServerInfoCard = () => {
  const [serverInfo, setServerInfo] = useState(null)
  const { showNotification } = useNotification();

  useEffect(() => {
    const fetchServerInfo = async () => {
      try {
        const info = await invoke('get_server_info');
        setServerInfo(info);
      } catch (error) {
        showNotification('error', 'Failed to load server info', error || 'An unknown error occurred');
        setServerInfo(null);
      }
    };
    fetchServerInfo();
  }, [showNotification]);
  return (
    <div>
      {serverInfo ? (
        <Card title="Server Info" icon={List}>
          <ul >
            <div className='grid grid-cols-2 gap-x-10 space-y-2'>
              <div className="space-y-2">
                <li className='flex justify-between'>
                  <span className="font-semibold">Server IP:</span> {serverInfo.server_ip}
                </li>
                <li className='flex justify-between'>
                  <span className="font-semibold">OS:</span> {serverInfo.os_name}
                </li>
                <li className='flex justify-between'>
                  <span className="font-semibold">Kernel:</span> {serverInfo.kernel_version}
                </li>
              </div>
              <div className="space-y-2">
                <li className='flex justify-between'>
                  <span className="font-semibold">Host:</span> {serverInfo.host_name}
                </li>
                <li className='flex justify-between'>
                  <span className="font-semibold">CPU cores:</span> {serverInfo.cpu_count}
                </li>

                <li className='flex justify-between'>
                  <span className="font-semibold">Usable Memory:</span> {(serverInfo.total_memory_mb / 1024 / 1024).toPrecision(4)} GB
                </li>
              </div>
            </div>
          </ul>
        </Card>
      ) : (
        <Card title="Server Info" icon={RefreshCw}>
          <div className="text-center py-4 text-gray-500">Loading server info...</div>
        </Card>
      )}
    </div>
  );
};

export default ServerInfoCard;