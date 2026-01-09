import { ShieldCheck, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { setupPrivilegedAccess } from "@/api/commands";
import { useToastStore } from "@/store/useToastStore";
import { Button, Card } from "@/components/ui";

export default function PrivilegeManagementForm() {
  const { success, error } = useToastStore();
  const [loading, setLoading] = useState(false);

  const handleSetup = async () => {
    setLoading(true);
    try {
      const response = await setupPrivilegedAccess({});
      success("Authorization", response.message);
    } catch (e) {
      error(e.toString());
    } finally {
      setLoading(false);
    }
  };

  return (
    <Card title="Privilege Management" subtitle="Authorize application to perform administrative tasks" icon={ShieldCheck}>
      <div className="space-y-4">
        <p className="text-sm text-base-content/60">
          Authorize the application to perform administrative tasks (service
          management, ZFS operations, package installation) without manual
          password prompts.
        </p>

        <div className="flex items-start gap-3 p-4 bg-warning/10 border border-warning/20 rounded-lg text-warning text-xs">
          <ShieldAlert size={20} className="shrink-0" />
          <p>
            This will create a specific sudoers rule for the current user. A
            one-time password prompt (Polkit) will appear to authorize this
            operation.
          </p>
        </div>

        <Button
          variant="primary"
          onClick={handleSetup}
          loading={loading}
          className="w-full"
        >
          Authorize Application
        </Button>
      </div>
    </Card>
  );
}
