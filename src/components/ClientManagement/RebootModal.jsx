import React, { useState } from "react";
import { RefreshCw } from "lucide-react";
import { useToastStore } from "@/store/useToastStore";
import { Button, Modal } from "@/components/ui";
import * as api from "@/api/commands";

const RebootModal = ({ client, isOpen, onClose, onSuccess }) => {
  const { success, error: showError } = useToastStore();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [mode, setMode] = useState("graceful");
  const [delayMinutes, setDelayMinutes] = useState(0);
  const [useScheduled, setUseScheduled] = useState(false);

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (!client) return;

    setIsSubmitting(true);
    try {
      const response = await api.rebootClient(client.id, {
        force: mode === "force",
        delay_minutes: useScheduled ? delayMinutes : null,
      });

      success(
        "Control Operations",
        response?.message || "Reboot command sent successfully"
      );
      onClose();
      if (onSuccess) {
        onSuccess();
      }
    } catch (err) {
      showError(
        "Control Operations",
        `Failed to reboot: ${err.message || String(err)}`
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    setMode("graceful");
    setDelayMinutes(0);
    setUseScheduled(false);
    onClose();
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Reboot Client"
      size="lg"
    >
      <form onSubmit={handleSubmit} className="space-y-6">
        <div className="bg-base-200/30 p-4 rounded-lg">
          <p className="text-sm text-base-content/70">
            Rebooting: <span className="font-semibold">{client?.name}</span>
          </p>
        </div>

        {/* Reboot Mode Selection */}
        <div className="space-y-3">
          <label className="label">
            <span className="label-text font-medium">Reboot Mode</span>
          </label>
          <div className="space-y-2">
            <label className="label cursor-pointer justify-start gap-3">
              <input
                type="radio"
                name="mode"
                value="graceful"
                checked={mode === "graceful"}
                onChange={(e) => setMode(e.target.value)}
                className="radio radio-primary"
              />
              <div className="flex flex-col">
                <span className="label-text font-medium">Graceful Reboot</span>
                <span className="label-text-alt text-xs text-base-content/60">
                  Allows running processes to terminate cleanly
                </span>
              </div>
            </label>
            <label className="label cursor-pointer justify-start gap-3">
              <input
                type="radio"
                name="mode"
                value="force"
                checked={mode === "force"}
                onChange={(e) => setMode(e.target.value)}
                className="radio radio-primary"
              />
              <div className="flex flex-col">
                <span className="label-text font-medium">Force Reboot</span>
                <span className="label-text-alt text-xs text-base-content/60">
                  Immediate reboot without waiting for processes
                </span>
              </div>
            </label>
          </div>
        </div>

        {/* Scheduled Operation */}
        <div className="space-y-3">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              checked={useScheduled}
              onChange={(e) => setUseScheduled(e.target.checked)}
              className="checkbox checkbox-primary"
            />
            <span className="label-text font-medium">Schedule Reboot</span>
          </label>

          {useScheduled && (
            <div className="ml-6 space-y-2">
              <label className="label">
                <span className="label-text text-sm">Delay (minutes)</span>
              </label>
              <input
                type="number"
                min="1"
                max="1440"
                value={delayMinutes}
                onChange={(e) => setDelayMinutes(Math.max(0, parseInt(e.target.value) || 0))}
                className="input input-bordered input-sm w-full"
                placeholder="Enter delay in minutes"
              />
              <p className="text-xs text-base-content/60">
                Client will reboot after {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
              </p>
            </div>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex justify-end gap-3 pt-4 border-t border-base-200/30">
          <Button
            type="button"
            variant="ghost"
            onClick={handleClose}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            icon={RefreshCw}
            disabled={isSubmitting}
          >
            {isSubmitting ? "Sending..." : "Reboot"}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default RebootModal;
