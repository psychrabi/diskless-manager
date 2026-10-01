import { ShieldCheck, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { setupPrivilegedAccess } from "@/api/modules/system";
import { useToastStore } from "@/store/useToastStore";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { FieldDescription } from "@/components/ui/field";
import { Button, Card } from "@/components/ui";

const AuthorizeStep = ({ onAuthorized, authorized = false, checking = false }) => {
  const { success, error } = useToastStore();
  const [loading, setLoading] = useState(false);
  const [setupError, setSetupError] = useState("");

  // When the backend probe confirms the user is already authorized the wizard
  // will auto-advance to the next step; show the disabled state while we wait.
  const isDisabled = authorized || loading || checking;

  useEffect(() => {
    if (!authorized || checking) return;
    // Give the UI a moment to reflect the disabled state before moving on.
    const timer = setTimeout(() => {
      onAuthorized();
    }, 400);
    return () => clearTimeout(timer);
  }, [authorized, checking, onAuthorized]);

  const handleAuthorize = async () => {
    if (authorized) return;
    setLoading(true);
    setSetupError("");
    try {
      const response = await setupPrivilegedAccess({});
      success("Authorization", response.message);
      onAuthorized();
    } catch (e) {
      setSetupError(e.message);
      error("Authorization failed", e.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Card
      title="Authorize Application"
      subtitle="Grant privileged access for system management"
      icon={ShieldCheck}
      className="w-full max-w-md mx-auto"
    >
      <div className="space-y-4">
        <FieldDescription>
          The application requires privileged access to manage system services,
          storage, and configuration files. This includes:
        </FieldDescription>

        <ul className="text-sm text-muted-foreground space-y-1.5 list-disc list-inside">
          <li>Service management (DHCP, TFTP, HTTP, Samba)</li>
          <li>ZFS storage pool operations</li>
          <li>Package installation and updates</li>
          <li>Network configuration</li>
        </ul>

        <Alert variant="default" className="border-amber-600/20 bg-amber-600/10 text-amber-600">
          <ShieldAlert />
          <AlertDescription className="text-amber-600">
            This creates a sudoers rule for the server account. The password
            prompt (Polkit) opens on the server computer. If no desktop
            authentication agent is available, use the terminal command shown
            below after a failed attempt, then retry authorization.
          </AlertDescription>
        </Alert>

        {setupError && (
          <Alert variant="destructive">
            <AlertDescription className="break-words whitespace-pre-wrap">{setupError}</AlertDescription>
          </Alert>
        )}

        <Button
          variant="primary"
          onClick={handleAuthorize}
          loading={loading}
          disabled={isDisabled}
          className="w-full"
        >
          {authorized
            ? "Already Authorized"
            : loading || checking
              ? "Checking..."
              : "Authorize Application"}
        </Button>
      </div>
    </Card>
  );
};

export default AuthorizeStep;
