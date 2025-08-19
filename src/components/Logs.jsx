import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '../store/useAppStore'

export const Logs = () => {
  const [logUnit, setLogUnit] = useState('');
  const [logs, setLogs] = useState(null);
  const [logLoading, setLogLoading] = useState(false);
  const services = useAppStore(state => state.services)

  const fetchLogs = async () => {
    if (!logUnit) return
    setLogLoading(true)
    try {
      const out = await invoke('get_service_logs', { unit: logUnit, lines: 200 })
      setLogs(out)
    } finally {
      setLogLoading(false)
    }
  }

  useEffect(() => {
    fetchLogs()
  }, [logUnit])

  return (
    <div className="card bg-base-200 md:col-span-2">
      <div className="card-body gap-4">
        <div className="flex items-center gap-2">
          <h2 className="card-title">Service logs</h2>
          <select className="select select-bordered select-sm w-full max-w-xs" value={logUnit} onChange={(e) => setLogUnit(e.target.value)}>
            <option value="">Select service unit</option>
            {Object.entries(services || {}).map(([key, svc]) => (
              <option key={key} value={svc.service}>{svc.name || svc.service}</option>
            ))}
          </select>
          <button className="btn btn-sm" onClick={fetchLogs} disabled={logLoading || !logUnit}>{logLoading ? 'Loading…' : 'Fetch'}</button>
        </div>
        <pre className="bg-base-300 p-3 rounded max-h-72 overflow-auto text-xs whitespace-pre-wrap">{logs || 'No logs yet.'}</pre>
      </div>
    </div>
  )
}