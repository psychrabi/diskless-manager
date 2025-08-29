import { useEffect, useState, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '../store/useAppStore'
import { Card, Select } from './ui';
import AppLogs from './Logs/AppLogs';

export const Logs = () => {
  const [logUnit, setLogUnit] = useState('');
  const [logs, setLogs] = useState(null);
  const services = useAppStore(state => state.services)

  const fetchLogs = useCallback(async () => {
    if (!logUnit) return
    try {
      const out = await invoke('get_service_logs', { unit: logUnit, lines: 200 })
      setLogs(out)
    } catch (error) {
      console.error(error)
    }
  }, [logUnit])

  useEffect(() => {
    fetchLogs()
  }, [fetchLogs])

  return (
    <Card title="Logs" actions={<Select  id="log-unit" value={logUnit} onChange={(e) => setLogUnit(e.target.value)}>
      <option value="">Select service unit</option>
      {Object.entries(services || {}).map(([key, svc]) => (
        <option key={key} value={svc.service}>{svc.name || svc.service}</option>
      ))}
    </Select>}>
      <pre className="bg-base-300 p-3 rounded max-h-[calc(100vh-14rem)] overflow-auto text-xs whitespace-pre-wrap">{logs}</pre>
      {!logs && <AppLogs />}
    </Card>
  )
}