import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { getFirewallStatus } from "@/api/modules/services";
import { useToastStore } from "@/store/useToastStore";
import { Copy, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";

// Warns when firewalld blocks boot-required traffic. The app cannot fix
// this itself (no firewall privileges), so it shows the exact commands.
const FirewallBanner = () => {
  const { success } = useToastStore();
  const [status, setStatus] = useState(null);

  useEffect(() => {
    let cancelled = false;
    getFirewallStatus().then(
      (data) => {
        if (!cancelled) setStatus(data);
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status || !status.firewall_running || status.missing_services?.length === 0) {
    return null;
  }

  const copyCommands = async () => {
    try {
      await navigator.clipboard.writeText(status.fix_commands.join("\n"));
      success("Firewall", "Fix commands copied to clipboard.");
    } catch {
      // Clipboard unavailable; commands remain visible below.
    }
  };

  return (
    <Alert variant="destructive">
      <ShieldAlert data-icon="inline-start" />
      <AlertTitle>Firewall is blocking required services</AlertTitle>
      <AlertDescription>
        <p>
          firewalld is running but not allowing:{" "}
          <span className=" font-medium">
            {(status.missing_services || []).join(", ")}
          </span>
          . Clients may fail to PXE-boot or mount disks until these are
          allowed. Run on the server:
        </p>
        <pre className="mt-2 overflow-x-auto rounded-md bg-muted p-2  text-xs">
          {(status.fix_commands || []).join("\n")}
        </pre>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="mt-2"
          onClick={copyCommands}
        >
          <Copy data-icon="inline-start" />
          Copy commands
        </Button>
      </AlertDescription>
    </Alert>
  );
};

export default FirewallBanner;
