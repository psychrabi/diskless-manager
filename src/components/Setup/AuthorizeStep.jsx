import { ShieldCheck, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { setupPrivilegedAccess } from "@/api/modules/system";
import { useToastStore } from "@/store/useToastStore";
import { Button, Card } from "@/components/ui";

const AuthorizeStep = ({ onAuthorized, authorized = false, checking = false }) => {
  const { success, error } = useToastStore();
  const [loading, setLoading] = useState(false);

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
    try {
      const response = await setupPrivilegedAccess({});
      success("Authorization", response.message);
      onAuthorized();
    } catch (e) {
      error(e.toString());
    } finally {
      setLoading(false);
    }
  };

  return (
    <Card className="w-full max-w-md mx-auto">
      <div className="text-center mb-6">
        <div className="flex justify-center mb-4">
          <div className="p-3 bg-primary/10 rounded-full">
            <ShieldCheck size={32} className="text-primary" />
          </div>
        </div>
        <h2 className="text-2xl font-bold text-primary">Authorize Application</h2>
        <p className="text-muted-foreground mt-2">
          Grant privileged access for system management
        </p>
      </div>

      <div className="space-y-4">
        <p className="text-sm text-muted-foreground leading-relaxed">
          The application requires privileged access to manage system services,
          storage, and configuration files. This includes:
        </p>

        <ul className="text-sm text-muted-foreground space-y-1.5 list-disc list-inside">
          <li>Service management (DHCP, TFTP, HTTP, Samba)</li>
          <li>ZFS storage pool operations</li>
          <li>Package installation and updates</li>
          <li>Network configuration</li>
        </ul>

        <div className="flex items-center gap-3 p-3 bg-amber-600/10 border border-amber-600/20 rounded-lg text-amber-600 text-xs">
          <ShieldAlert size={24} className="shrink-0" />
          <p>
            A one-time password prompt (Polkit) will appear to authorize this
            operation. This creates a specific sudoers rule for the current user.
          </p>
        </div>

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
