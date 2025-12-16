import { useEffect, useState } from "react";
import { useServiceManager } from "../../hooks/useServiceManager";
import { Button, Modal } from "../ui";

function ServiceConfigModal({
  isOpen,
  onClose,
  title,
  serviceKey,
  initialConfig,
  initialLoading,
  path,
}) {
  const [config, setConfig] = useState(initialConfig || "");
  const [editable, setEditable] = useState(false);
  const [saving, setSaving] = useState(false);
  const { handleConfigSave } = useServiceManager();

  // Sync prop config to local state when it changes (e.g. data fetch completes)
  useEffect(() => {
    setConfig(initialConfig || "");
  }, [initialConfig]);

  // Reset editable state when modal opens/closes
  useEffect(() => {
    if (!isOpen) {
      setEditable(false);
    }
  }, [isOpen]);

  const handleChange = (e) => {
    setConfig(e.target.value);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await handleConfigSave(serviceKey, config);
      setEditable(false);
      onClose();
    } catch (error) {
      // Error is handled by toast in useServiceManager, but we need to stop loading
      console.error(error);
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setEditable(false);
    setConfig(initialConfig || ""); // Reset to initial on cancel
    onClose();
  };

  return (
    <Modal isOpen={isOpen} onClose={handleCancel} title={title} size="5xl">
      <h3>Configuration path: {path}</h3>
      {initialLoading ? (
        <div className="flex justify-center items-center h-40">
          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
        </div>
      ) : editable ? (
        <textarea
          className="bg-base-100 p-4 rounded-md text-xs overflow-auto h-[80vh] w-full font-mono max-h-[80vh]"
          value={config}
          onChange={handleChange}
          disabled={saving}
          spellCheck={false}
        />
      ) : (
        <pre className="bg-base-100 p-4 rounded-md text-xs overflow-auto h-[80vh] max-h-[80vh]">
          <code>{config}</code>
        </pre>
      )}
      <div className="mt-4 flex justify-end space-x-2">
        {editable ? (
          <Button
            onClick={() => handleSave()}
            loading={saving}
            disabled={saving || initialLoading}
          >
            Save
          </Button>
        ) : (
          <Button
            onClick={() => setEditable(true)}
            disabled={saving || initialLoading}
          >
            Edit
          </Button>
        )}
        <Button
          variant="outline"
          onClick={() => handleCancel()}
          disabled={saving}
        >
          Cancel
        </Button>
      </div>
    </Modal>
  );
}

export default ServiceConfigModal;
