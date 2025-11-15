import { invoke } from '@tauri-apps/api/core';
import { Activity, useCallback, useEffect, useState } from 'react';
import { useAppStore } from '../store/useAppStore';
import AppLogs from './Logs/AppLogs';
import { Card, Select } from './ui';

const Logs = () => {
  const [logUnit, setLogUnit] = useState('app_log');
  const [logs, setLogs] = useState(null);
  const services = useAppStore(state => state.services)

  const fetchLogs = useCallback(async () => {
    if (!logUnit) return
    try {
      const out = await invoke('get_service_logs', { unit: logUnit, lines: 50 })
      setLogs(out)
    } catch (error) {
      console.error(error)
    }
  }, [logUnit])

  useEffect(() => {
    const getLogs = async () => {
      await fetchLogs();
    };
    getLogs();
  }, [fetchLogs])

  const logOptions = <Select id="log-unit" value={logUnit} onChange={(e) => setLogUnit(e.target.value)}>
    <option value="app_log">Show App logs</option>
    {Object.entries(services || {}).map(([key, svc]) => (
      <option key={key} value={svc.service}>{svc.name || svc.service}</option>
    ))}
  </Select>;

  return (
    <Card title="Logs" actions={logOptions} className='max-h-[calc(100vh-7rem)]'>
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