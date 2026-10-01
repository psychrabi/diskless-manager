import { ShieldCheck, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { checkPrivilegedAccess, setupPrivilegedAccess } from "@/api/modules/system";
import { useToastStore } from "@/store/useToastStore";
import { Button, Card } from "@/components/ui";
import { Alert, AlertDescription } from "@/components/ui/alert";

export default function PrivilegeManagementForm() {
  const { success, error } = useToastStore();
  const [loading, setLoading] = useState(false);
  const [authorized, setAuthorized] = useState(false);
  const [checking, setChecking] = useState(true);
  const [setupError, setSetupError] = useState("");

  useEffect(() => {
    let cancelled = false;
    checkPrivilegedAccess()
      .then((result) => { if (!cancelled) setAuthorized(result.authorized); })
      .catch((error) => { if (!cancelled) setSetupError(error.message); })
      .finally(() => { if (!cancelled) setChecking(false); });
    return () => { cancelled = true; };
  }, []);

  const handleSetup = async () => {
    if (authorized) return;
    setLoading(true);
    setSetupError("");
    try {
      const response = await setupPrivilegedAccess({});
      success("Authorization", response.message);
      setAuthorized(true);
    } catch (e) {
      setSetupError(e.message);
      error("Authorization failed", e.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Card
      title="Privilege Management"
      subtitle="Authorize application to perform administrative tasks"
      icon={ShieldCheck}
      className="h-full"
    >
      <div className="space-y-4">
        <p className="text-sm text-muted-foreground">
          Authorize the application to perform administrative tasks (service
          management, ZFS operations, package installation) without manual
          password prompts.
        </p>

        <div className="flex items-center gap-3 p-3 bg-amber-600/10 border border-amber-600/20 rounded-lg text-amber-600 text-xs">
          <ShieldAlert size={24} className="shrink-0" />
          <p>
            This will create a specific sudoers rule for the current user. A
            one-time password prompt (Polkit) opens on the server computer. If
            the server has no desktop authentication agent, use the terminal
            command shown below after a failed attempt.
          </p>
        </div>

        {setupError && (
          <Alert variant="destructive">
            <AlertDescription className="break-words whitespace-pre-wrap">{setupError}</AlertDescription>
          </Alert>
        )}

        <div className="flex justify-end">
          <Button
            variant="primary"
            onClick={handleSetup}
            loading={loading}
            disabled={authorized || loading || checking}
          >
            {authorized ? "Already Authorized" : checking ? "Checking..." : "Authorize Application"}
          </Button>
        </div>
      </div>
    </Card>
  );
}
