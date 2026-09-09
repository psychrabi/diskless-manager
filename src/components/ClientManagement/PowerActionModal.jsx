import { useState } from "react";
import { Power, RefreshCw } from "lucide-react";
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
import { Alert, AlertDescription } from "@/components/ui/alert";
import { FieldDescription } from "@/components/ui/field";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";

const CONFIG = {
  shutdown: {
    title: "Shutdown Client",
    label: "Shutdown",
    verb: "shutdown",
    verbing: "Shutting down",
    noun: "Shutdown",
    icon: Power,
    apiCall: (id, opts) => shutdownClient(id, opts),
  },
  reboot: {
    title: "Reboot Client",
    label: "Reboot",
    verb: "reboot",
    verbing: "Rebooting",
    noun: "Reboot",
    icon: RefreshCw,
    apiCall: (id, opts) => rebootClient(id, opts),
  },
};

const PowerActionModal = ({ client, isOpen, onClose, onSuccess, type = "shutdown" }) => {
  const { success, error: showError } = useToastStore();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [mode, setMode] = useState("graceful");
  const [delayMinutes, setDelayMinutes] = useState(0);
  const [useScheduled, setUseScheduled] = useState(false);

  const cfg = CONFIG[type] || CONFIG.shutdown;

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (!client) return;

    setIsSubmitting(true);
    try {
      const response = await cfg.apiCall(client.id, {
        force: mode === "force",
        delay_minutes: useScheduled ? delayMinutes : null,
      });

      success(
        "Control Operations",
        response?.message || `${cfg.label} command sent successfully`
      );
      onClose();
      if (onSuccess) {
        onSuccess();
      }
    } catch (err) {
      showError(
        "Control Operations",
        `Failed to ${cfg.verb}: ${err.message || String(err)}`
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

  const ModeIcon = cfg.icon;

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) handleClose();
      }}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{cfg.title}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <Alert>
            <AlertDescription>
              {cfg.verbing}: <span className="font-semibold text-foreground">{client?.name}</span>
            </AlertDescription>
          </Alert>

          <div className="flex flex-col gap-2">
            <Label className="font-medium">{cfg.noun} Mode</Label>
            <RadioGroup value={mode} onValueChange={(value) => setMode(value)} className="gap-3">
              <Label className="flex cursor-pointer items-start gap-3">
                <RadioGroupItem value="graceful" />
                <div className="flex flex-col">
                  <span className="font-medium">Graceful {cfg.noun}</span>
                  <span className="text-xs text-muted-foreground">
                    Allows running processes to terminate cleanly
                  </span>
                </div>
              </Label>
              <Label className="flex cursor-pointer items-start gap-3">
                <RadioGroupItem value="force" />
                <div className="flex flex-col">
                  <span className="font-medium">Force {cfg.noun}</span>
                  <span className="text-xs text-muted-foreground">
                    Immediate {cfg.verb} without waiting for processes
                  </span>
                </div>
              </Label>
            </RadioGroup>
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-3">
              <Checkbox
                id="schedule-action"
                checked={useScheduled}
                onCheckedChange={(checked) => setUseScheduled(Boolean(checked))}
              />
              <Label htmlFor="schedule-action" className="cursor-pointer font-medium">
                Schedule {cfg.noun}
              </Label>
            </div>

            {useScheduled && (
              <div className="ml-6 flex flex-col gap-2">
                <Label htmlFor="delay-minutes" className="text-sm">
                  Delay (minutes)
                </Label>
                <Input
                  id="delay-minutes"
                  type="number"
                  min="1"
                  max="1440"
                  value={delayMinutes}
                  onChange={(e) => setDelayMinutes(Math.max(0, parseInt(e.target.value) || 0))}
                  placeholder="Enter delay in minutes"
                />
                <FieldDescription>
                  Client will {cfg.verb} after {delayMinutes} minute{delayMinutes !== 1 ? "s" : ""}
                </FieldDescription>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={handleClose}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <ModeIcon data-icon="inline-start" />
              )}
              {isSubmitting ? "Sending..." : cfg.label}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default PowerActionModal;
