import { useState } from "react";
import { Clock } from "lucide-react";
import { useToastStore } from "@/store/useToastStore";
import { Button, Modal } from "@/components/ui";
import { shutdownClient, rebootClient } from "@/api/modules/control";

const ScheduledOperationModal = ({ client, isOpen, onClose, onSuccess }) => {
  const { success, error: showError } = useToastStore();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [operationType, setOperationType] = useState("shutdown");
  const [mode, setMode] = useState("graceful");
  const [delayMinutes, setDelayMinutes] = useState(5);

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (!client || delayMinutes < 1) return;

    setIsSubmitting(true);
    try {
      let response;
      if (operationType === "shutdown") {
        response = await shutdownClient(client.id, {
          force: mode === "force",
          delay_minutes: delayMinutes,
        });
      } else if (operationType === "reboot") {
        response = await rebootClient(client.id, {
          force: mode === "force",
          delay_minutes: delayMinutes,
        });
      }

      success(
        "Control Operations",
        response?.message || `${operationType} scheduled successfully`
      );
      onClose();
      if (onSuccess) {
        onSuccess();
      }
    } catch (err) {
      showError(
        "Control Operations",
        `Failed to schedule ${operationType}: ${err.message || String(err)}`
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    setOperationType("shutdown");
    setMode("graceful");
    setDelayMinutes(5);
    onClose();
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={handleClose}
      title="Schedule Operation"
      size="lg"
    >
      <form onSubmit={handleSubmit} className="space-y-6">
        <div className="bg-base-200/30 p-4 rounded-lg">
          <p className="text-sm text-base-content/70">
            Scheduling operation for:{" "}
            <span className="font-semibold">{client?.name}</span>
          </p>
        </div>

        {/* Operation Type Selection */}
        <div className="space-y-3">
          <label className="label">
            <span className="label-text font-medium">Operation Type</span>
          </label>
          <div className="space-y-2">
            <label className="label cursor-pointer justify-start gap-3">
              <input
                type="radio"
                name="operationType"
                value="shutdown"
                checked={operationType === "shutdown"}
                onChange={(e) => setOperationType(e.target.value)}
                className="radio radio-primary"
              />
              <span className="label-text">Shutdown</span>
            </label>
            <label className="label cursor-pointer justify-start gap-3">
              <input
                type="radio"
                name="operationType"
                value="reboot"
                checked={operationType === "reboot"}
                onChange={(e) => setOperationType(e.target.value)}
                className="radio radio-primary"
              />
              <span className="label-text">Reboot</span>
            </label>
          </div>
        </div>

        {/* Operation Mode Selection */}
        <div className="space-y-3">
          <label className="label">
            <span className="label-text font-medium">Operation Mode</span>
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
              <span className="label-text">Graceful</span>
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
              <span className="label-text">Force</span>
            </label>
          </div>
        </div>

        {/* Delay Input */}
        <div className="space-y-3">
          <label className="label">
            <span className="label-text font-medium">Delay (minutes)</span>
          </label>
          <input
            type="number"
            min="1"
            max="1440"
            value={delayMinutes}
            onChange={(e) => setDelayMinutes(Math.max(1, parseInt(e.target.value) || 1))}
            className="input input-bordered w-full"
            placeholder="Enter delay in minutes"
          />
          <p className="text-xs text-base-content/60">
            Operation will execute after {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
          </p>
        </div>

        {/* Summary */}
        <div className="bg-info/10 border border-info/30 p-4 rounded-lg">
          <p className="text-sm text-info-content">
            <strong>Summary:</strong> {mode === "graceful" ? "Graceful" : "Force"}{" "}
            {operationType} in {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
          </p>
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
            icon={Clock}
            disabled={isSubmitting || delayMinutes < 1}
          >
            {isSubmitting ? "Scheduling..." : "Schedule"}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default ScheduledOperationModal;
