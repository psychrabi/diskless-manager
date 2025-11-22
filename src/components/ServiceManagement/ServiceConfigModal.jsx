import { useState } from 'react'
import { useServiceManager } from '../../hooks/useServiceManager'
import { useAppStore } from '../../store/useAppStore'
import { Button, Modal } from '../ui'

function ServiceConfigModal() {
  const title = useAppStore(state => state.title)
  const loading = useAppStore(state => state.loading)
  const config = useAppStore(state => state.serviceConfig)
  const setConfig = useAppStore(state => state.setServiceConfig)
  const open = useAppStore(state => state.open)
  const setOpen = useAppStore(state => state.setOpen)
  const [editable, setEditable] = useState(false)
  const [saving, setSaving] = useState(false)
  const { handleConfigSave } = useServiceManager()
  const serviceKey = useAppStore(state => state.serviceKey)

  const handleChange = e => {
    setConfig(e.target.value)
  }

  const handleSave = () => {
    setSaving(true)
    handleConfigSave(serviceKey, config)
    setSaving(false)
  }

  const handleCancel = () => {
    setEditable(false)
    setOpen(false)
  }

  return (
    <Modal isOpen={open} onClose={() => setOpen(false)} title={title} size="5xl">
      {loading ? (
        <div className="flex justify-center items-center h-40">
          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
        </div>
      ) : (
        editable ? (
          <textarea
            className="bg-base-100 p-4 rounded-md text-xs overflow-auto h-[80vh] w-full font-mono max-h-[80vh]"
            value={config}
            onChange={handleChange}
            disabled={saving}
            spellCheck={false}
          />
        ) : (
          <pre className="bg-base-100 p-4 rounded-md text-xs overflow-auto h-[80vh] max-h-[80vh]" >
            <code>{config}</code>
          </pre>
        )
      )}
      <div className="mt-4 flex justify-end space-x-2">
        {editable ? (
          <Button onClick={() => handleSave()} loading={saving} disabled={saving || loading}>Save</Button>
        ) :
          <Button onClick={() => setEditable(true)} disabled={saving}>Edit</Button>
        }
        <Button variant="outline" onClick={() => handleCancel()} disabled={saving}>Cancel</Button>
      </div>
    </Modal>
  )
}

export default ServiceConfigModal