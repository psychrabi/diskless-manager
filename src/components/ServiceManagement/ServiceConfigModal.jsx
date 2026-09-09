import { Textarea } from "@/components/ui/textarea";
import { DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { useState } from "react";
import { useServiceManager } from "../../hooks/useServiceManager";
import { Button, Modal } from "@/components/ui";

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

  // Sync local config when the fetched value from the parent changes.
  // Adjusting state during render avoids a setState-in-effect cascade
  // (react.dev: "adjusting state when a prop changes").
  const normalizedInitialConfig = initialConfig || "";
  const [prevInitialConfig, setPrevInitialConfig] = useState(
    normalizedInitialConfig
  );
  if (normalizedInitialConfig !== prevInitialConfig) {
    setPrevInitialConfig(normalizedInitialConfig);
    setConfig(normalizedInitialConfig);
  }

  // Reset edit mode whenever the modal closes.
  const [prevIsOpen, setPrevIsOpen] = useState(isOpen);
  if (prevIsOpen !== isOpen) {
    setPrevIsOpen(isOpen);
    if (!isOpen) {
      setEditable(false);
    }
  }

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
      <DialogDescription>Configuration path: {path}</DialogDescription>
      {initialLoading ? (
        <div className="flex justify-center items-center h-40">
          <Spinner className="size-8" />
        </div>
      ) : editable ? (
        <Textarea
          aria-label="Service configuration"
          className="bg-background p-4 rounded-md text-xs overflow-auto h-[80vh] w-full  max-h-[80vh]"
          value={config}
          onChange={handleChange}
          disabled={saving}
          spellCheck={false}
        />
      ) : (
        <pre className="bg-background p-4 rounded-md text-xs overflow-auto h-[80vh] max-h-[80vh]">
          <code>{config}</code>
        </pre>
      )}
      <DialogFooter className="mt-4">
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
        </DialogFooter>
      </Modal>
  );
}

export default ServiceConfigModal;
