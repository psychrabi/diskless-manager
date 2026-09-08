import { useState } from "react";
import { Clock } from "lucide-react";
import { useToastStore } from "@/store/useToastStore";
import { shutdownClient, rebootClient } from "@/api/modules/control";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Spinner } from "@/components/ui/spinner";

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
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) handleClose();
      }}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Schedule Operation</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="rounded-lg bg-muted p-4">
            <p className="text-sm text-muted-foreground">
              Scheduling operation for:{" "}
              <span className="font-semibold text-foreground">{client?.name}</span>
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Label className="font-medium">Operation Type</Label>
            <RadioGroup
              value={operationType}
              onValueChange={(value) => setOperationType(value)}
              className="gap-2"
            >
              <Label className="flex cursor-pointer items-center gap-3">
                <RadioGroupItem value="shutdown" />
                <span>Shutdown</span>
              </Label>
              <Label className="flex cursor-pointer items-center gap-3">
                <RadioGroupItem value="reboot" />
                <span>Reboot</span>
              </Label>
            </RadioGroup>
          </div>

          <div className="flex flex-col gap-2">
            <Label className="font-medium">Operation Mode</Label>
            <RadioGroup
              value={mode}
              onValueChange={(value) => setMode(value)}
              className="gap-2"
            >
              <Label className="flex cursor-pointer items-center gap-3">
                <RadioGroupItem value="graceful" />
                <span>Graceful</span>
              </Label>
              <Label className="flex cursor-pointer items-center gap-3">
                <RadioGroupItem value="force" />
                <span>Force</span>
              </Label>
            </RadioGroup>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="delay-minutes" className="font-medium">
              Delay (minutes)
            </Label>
            <Input
              id="delay-minutes"
              type="number"
              min="1"
              max="1440"
              value={delayMinutes}
              onChange={(e) => setDelayMinutes(Math.max(1, parseInt(e.target.value) || 1))}
              placeholder="Enter delay in minutes"
            />
            <p className="text-xs text-muted-foreground">
              Operation will execute after {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
            </p>
          </div>

          <div className="rounded-lg border border-border bg-muted/40 p-4">
            <p className="text-sm text-muted-foreground">
              <strong className="text-foreground">Summary:</strong> {mode === "graceful" ? "Graceful" : "Force"}{" "}
              {operationType} in {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
            </p>
          </div>

          <DialogFooter className="pt-2">
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
              disabled={isSubmitting || delayMinutes < 1}
            >
              {isSubmitting ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <Clock data-icon="inline-start" />
              )}
              {isSubmitting ? "Scheduling..." : "Schedule"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default ScheduledOperationModal;
