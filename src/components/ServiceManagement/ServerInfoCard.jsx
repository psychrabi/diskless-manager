import { invoke } from '@tauri-apps/api/core';
import { List, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Card } from '../ui';


// type ServerInfo = {
//   os_name: string | null
//   kernel_version: string | null
//   host_name: string | null
//   total_memory_mb: number
//   cpu_count: number
// }

const ServerInfoCard = () => {
  const [serverInfo, setServerInfo] = useState(null)

  useEffect(() => {
    ; (async () => {
      const info = await invoke('get_server_info')
      setServerInfo(info)
    })()
  }, [])
  return (
    <div>
      {serverInfo ? (
        <Card title="Server Info" icon={List}>
          <ul className="space-y-2">
            <li className='flex justify-between items-center'>
              <span className="font-semibold">Host:</span> {serverInfo.host_name}
            </li>
            <li className='flex justify-between items-center'>
              <span className="font-semibold">OS:</span> {serverInfo.os_name}
            </li>
            <li className='flex justify-between items-center'>
              <span className="font-semibold">Kernel:</span> {serverInfo.kernel_version}
            </li>
            <li className='flex justify-between items-center'>
              <span className="font-semibold">CPU cores:</span> {serverInfo.cpu_count}
            </li>
            <li className='flex justify-between items-center'>
              <span className="font-semibold">Usable Memory:</span> {(serverInfo.total_memory_mb / 1024 / 1024).toPrecision(4)} GB
            </li>
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