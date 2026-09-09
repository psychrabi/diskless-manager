import { Badge } from "@/components/ui/badge";
import { Button, Card } from "@/components/ui";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useSettings } from "@/hooks/useSettings";
import { useToastStore } from "@/store/useToastStore";
import { closeEnrollment, getSettings, openEnrollment } from "@/api/modules/system";
import { DoorClosed, DoorOpen, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

const REFRESH_INTERVAL_MS = 30000;

const formatOpenUntil = (openUntil) => {
  if (!openUntil) return "";
  const date = new Date(openUntil * 1000);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString();
};

// Registration window control: unknown PXE machines may only
// self-register while the window is open, and land disabled (pool)
// until an administrator enables and provisions them.
const EnrollmentControl = () => {
  const { success, error } = useToastStore();
  const settings = useSettings();
  const [enrollment, setEnrollment] = useState(null);
  const [minutes, setMinutes] = useState(15);
  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);

  const applySettings = useCallback((current) => {
    setEnrollment(current?.enrollment ?? null);
    if (current?.enrollment?.window_minutes) {
      setMinutes(current.enrollment.window_minutes);
    }
    setNow(Date.now());
  }, []);

  const refresh = useCallback(async () => {
    try {
      applySettings(await getSettings());
    } catch {
      // Keep the last known status; a failed refresh must not wipe it.
      setNow(Date.now());
    }
  }, [applySettings]);

  useEffect(() => {
    let cancelled = false;
    getSettings().then(
      (current) => {
        if (!cancelled) applySettings(current);
      },
      () => {},
    );
    const timer = setInterval(() => {
      getSettings().then(
        (current) => {
          applySettings(current);
        },
        () => {},
      );
    }, REFRESH_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [applySettings]);

  const isOpen = (enrollment?.open_until ?? 0) * 1000 > now;

  const handleOpen = async () => {
    setBusy(true);
    try {
      const result = await openEnrollment(minutes);
      success("Enrollment", result?.message || "Enrollment opened.");
      await refresh();
    } catch (e) {
      error("Enrollment", e);
    } finally {
      setBusy(false);
    }
  };

  const handleClose = async () => {
    setBusy(true);
    try {
      const result = await closeEnrollment();
      success("Enrollment", result?.message || "Enrollment closed.");
      await refresh();
    } catch (e) {
      error("Enrollment", e);
    } finally {
      setBusy(false);
    }
  };

  const handleSaveMinutes = async () => {
    const parsed = Number(minutes);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 60) {
      error("Enrollment", "Window length must be between 1 and 60 minutes.");
      return;
    }
    await settings.updateEnrollment({ window_minutes: parsed });
    await refresh();
  };

  return (
    <Card
      title="Client Enrollment"
      subtitle="Registration window for unknown machines"
      icon={ShieldCheck}
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <Badge variant={isOpen ? "default" : "secondary"}>
            {isOpen ? "Open" : "Closed"}
          </Badge>
          <span className="text-sm text-muted-foreground">
            {isOpen
              ? `Unknown machines may register until ${formatOpenUntil(enrollment?.open_until)}.`
              : "Unknown machines are denied; registered machines appear disabled until enabled."}
          </span>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="enrollment-minutes">Window (minutes)</Label>
            <Input
              id="enrollment-minutes"
              type="number"
              min={1}
              max={60}
              className="w-24"
              value={minutes}
              disabled={busy}
              onChange={(e) => setMinutes(e.target.value)}
              onBlur={handleSaveMinutes}
            />
          </div>
          {isOpen ? (
            <Button
              type="button"
              variant="ghost"
              icon={DoorClosed}
              disabled={busy}
              onClick={handleClose}
            >
              Close now
            </Button>
          ) : (
            <Button
              type="button"
              variant="primary"
              icon={DoorOpen}
              disabled={busy}
              onClick={handleOpen}
            >
              Open enrollment
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
};

export default EnrollmentControl;
