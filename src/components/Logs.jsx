import { useLogs } from '@/hooks/useLogs';
import { RefreshCw } from 'lucide-react';
import { Activity, useEffect, useState } from 'react';
import { useAppStore } from '../store/useAppStore';
import AppLogs from './Logs/AppLogs';
import { Button, Card, Select } from './ui';

const Logs = () => {
  const [logUnit, setLogUnit] = useState('app_log');
  const services = useAppStore(state => state.services)
  const { logs, fetchLogs } = useLogs();

  useEffect(() => {
    if (logUnit) {
      fetchLogs(logUnit);
    }
  }, [logUnit, fetchLogs]);

  const logOptions = (
    <div className="flex gap-2">
      <Select id="log-unit" value={logUnit} onChange={(e) => setLogUnit(e.target.value)}>
        <option value="app_log">Show App logs</option>
        {Object.entries(services || {}).map(([key, svc]) => (
          <option key={key} value={svc.service}>{svc.name || svc.service}</option>
        ))}
      </Select>
      <Button variant="ghost" size="icon" onClick={() => fetchLogs(logUnit)} title="Refresh Logs" icon={RefreshCw} />
    </div>
  );

  return (
    <Card title="Logs" headerClass="p-4" actions={logOptions} className='max-h-[calc(100vh-7rem)]'>
      <Activity mode={logUnit !== 'app_log' ? 'visible' : 'hidden'}>
        <Card title={`${logUnit} Logs`} className="bg-base-200" headerClass="p-4" bodyClass="border-t-1">
          <pre className="bg-base-300 p-2 rounded  overflow-auto text-xs whitespace-pre-wrap max-h-[calc(100vh-20rem)]">
            {logs}
          </pre>
        </Card>
      </Activity>
      <Activity mode={logUnit === 'app_log' ? "visible" : "hidden"}>
        <AppLogs />
      </Activity>
    </Card>
  );
};

export default Logs;