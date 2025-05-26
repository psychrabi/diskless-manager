import React, { useState, useEffect } from 'react'
import { Button } from '../ui'
import { Modal } from '../ui'
import { useAppStore } from '../../store/useAppStore'
import { useServiceManager } from '../../hooks/useServiceManager'

function ServiceConfigModal({serviceKey}) {
  const title = useAppStore(state => state.title)
  const loading = useAppStore(state => state.loading)

  const config = useAppStore(state => state.config)
  const setConfig = useAppStore(state => state.setConfig)
  const open = useAppStore(state => state.open)
  const setOpen = useAppStore(state => state.setOpen)
  const [editable, setEditable] = useState(false)
  const [saving, setSaving] = useState(false)
  const {handleConfigSave} = useServiceManager()
 

  const handleChange = e => {
    setConfig(e.target.value)
    
  }

  const handleSave = () => {
    setSaving(true)
    handleConfigSave(serviceKey, config)
      setSaving(false)
  }

  return (
    <Modal isOpen={open} onClose={() => setOpen(false)} title={title} size="3xl">
      {loading ? (
        <div className="flex justify-center items-center h-40">
          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
        </div>
      ) : (
        editable ? (
          <textarea
            className="bg-gray-100 dark:bg-gray-900 p-4 rounded-md text-xs overflow-auto max-h-[70vh] w-full min-h-[300px] font-mono"
            value={config}
            onChange={handleChange}
            disabled={saving}
            spellCheck={false}
          />
        ) : (
          <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-md text-xs overflow-auto max-h-[70vh]" onClick={() => setEditable(true)}>
            <code>{config}</code>
          </pre>
        )
      )}
      <div className="mt-4 flex justify-end space-x-2">
        {editable && (
          <Button onClick={() => handleSave()} loading={saving} disabled={saving || loading}>Save</Button>
        )}
        <Button variant="outline" onClick={() => setOpen(false)} disabled={saving}>Cancel</Button>

      </div>
    </Modal>
  )
}

export default ServiceConfigModal