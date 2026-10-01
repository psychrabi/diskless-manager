import { useId, useState } from "react";
import { useAuth } from "@/contexts/auth";
import { useConfirm } from "@/contexts/confirmDialog";
import { downloadBackup, restoreBackup } from "@/api/modules/backup";
import { Button, Card } from "@/components/ui";
import { Label } from "@/components/ui/label";

export default function BackupManagement() {
  const { user } = useAuth();
  const confirm = useConfirm();
  const fileId = useId();
  const [file, setFile] = useState(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  if (user?.role !== "admin") return null;

  async function handleDownload() {
    setBusy("download");
    setError("");
    setMessage("");
    try {
      await downloadBackup();
      setMessage("Backup downloaded. Store this file securely: it contains authentication secrets.");
    } catch (error) {
      setError(error.message);
    } finally {
      setBusy("");
    }
  }

  async function handleRestore() {
    if (!file || busy) return;
    setBusy("restore");
    try {
      if (!await confirm({
        title: "Restore application backup?",
        description: "This replaces the application database, configuration, and authentication secrets after the backend restarts. A safety backup is taken before applying the restore. You may need to sign in with an account from the original backup. ZFS datasets and OS image disk contents are not restored.",
        confirmText: "Stage restore",
        confirmVariant: "destructive",
      })) return;
      setError("");
      setMessage("");
      const result = await restoreBackup(file);
      setMessage(`${result.message} Restart the backend to apply the restore. You may need to sign in with an original backup account.${result.safety_backup ? ` Safety backup: ${result.safety_backup}` : ""}`);
    } catch (error) {
      setError(error.message);
    } finally {
      setBusy("");
    }
  }

  return (
    <Card title="Application backup and restore" subtitle="Save or recover application data">
      <div className="space-y-4">
        <p className="text-sm text-muted-foreground">
          Backups include users, clients, image metadata, settings, licenses, configuration files, and authentication secrets.
          ZFS datasets and OS image disk contents are excluded. Store backups securely.
        </p>
        <Button onClick={handleDownload} disabled={Boolean(busy)} loading={busy === "download"}>Download backup</Button>
        <div className="space-y-2">
          <Label htmlFor={fileId}>Backup file</Label>
          <input id={fileId} type="file" accept=".json,application/json" disabled={Boolean(busy)}
            className="block w-full text-sm file:mr-3 file:rounded-md file:border file:border-input file:bg-background file:px-3 file:py-2 file:text-foreground"
            onChange={(event) => setFile(event.target.files?.[0] || null)} />
          <p className="text-sm text-muted-foreground">Choose an application backup JSON file (maximum 64 MiB). Restore applies after restarting the backend. You may need to sign in with an account from the original backup.</p>
        </div>
        <Button variant="destructive" onClick={handleRestore} disabled={!file || Boolean(busy)} loading={busy === "restore"}>Restore on restart</Button>
        {error && <p role="alert" className="break-words text-sm text-destructive">{error}</p>}
        {message && <p role="status" className="break-words text-sm">{message}</p>}
      </div>
    </Card>
  );
}
