import { ShieldCheck, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useToastStore } from "@/store/useToastStore";
import { Button, Card } from "../ui";

export default function PrivilegeManagementForm() {
  const { success, error, info } = useToastStore();
  const [loading, setLoading] = useState(false);

  const handleSetup = async () => {
    setLoading(true);
    try {
      info("Requesting administrative authorization...");
      const message = await invoke("setup_privileged_access");
      success(message);
    } catch (e) {
      error(e.toString());
    } finally {
      setLoading(false);
    }
  };

  return (
    <Card title="Privilege Management" icon={ShieldCheck}>
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
