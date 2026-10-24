import { useCallback, useEffect, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button, Input, PageHeader } from "@/components/ui";
import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { deleteInstallerMedia, listInstallerMedia, setDefaultInstallerMedia, uploadInstallerMedia } from "@/api/modules/installerMedia";
import { getSettings } from "@/api/modules/system";

const mediaTypes = [
  { value: "winpe", label: "WinPE" },
  { value: "linux_live", label: "Linux live" },
];

function formatBytes(value) {
  if (!Number.isFinite(value)) return "Unknown";
  if (value < 1024 ** 2) return `${value} bytes`;
  return `${(value / 1024 ** 2).toFixed(1)} MiB`;
}

export default function InstallerMedia() {
  const { success, error: toastError } = useToastStore();
  const confirm = useConfirm();
  const [media, setMedia] = useState([]);
  const [maxBytes, setMaxBytes] = useState(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [mediaType, setMediaType] = useState("winpe");
  const [file, setFile] = useState(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [catalog, settings] = await Promise.all([listInstallerMedia(), getSettings()]);
      setMedia(catalog);
      setMaxBytes(settings?.installer_media?.max_upload_bytes ?? null);
    } catch (err) {
      toastError("Installer media", err.message || "Could not load installer media.");
    } finally {
      setLoading(false);
    }
  }, [toastError]);

  useEffect(() => {
    const timer = setTimeout(() => { void load(); }, 0);
    return () => clearTimeout(timer);
  }, [load]);

  const runAction = async (action, successMessage) => {
    setBusy(true);
    try {
      await action();
      success("Installer media", successMessage);
      await load();
    } catch (err) {
      toastError("Installer media", err.message || "The operation failed.");
    } finally {
      setBusy(false);
    }
  };

  const handleUpload = async (event) => {
    event.preventDefault();
    if (!file || !displayName.trim()) {
      toastError("Installer media", "Enter a display name and choose an ISO file.");
      return;
    }
    if (!file.name.toLowerCase().endsWith(".iso")) {
      toastError("Installer media", "Choose a file with the .iso extension.");
      return;
    }
    if (maxBytes != null && file.size > maxBytes) {
      toastError("Installer media", `This file exceeds the configured limit of ${formatBytes(maxBytes)}.`);
      return;
    }
    await runAction(async () => {
      await uploadInstallerMedia({ displayName: displayName.trim(), mediaType, file });
      setDisplayName("");
      setFile(null);
    }, "Installer ISO uploaded and validated.");
  };

  const handleDelete = async (item) => {
    const approved = await confirm({
      title: "Delete installer ISO",
      description: `Delete ${item.display_name}? Media in use or selected as a default will be refused.`,
      confirmText: "Delete",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });
    if (approved) await runAction(() => deleteInstallerMedia(item.id), `${item.display_name} deleted.`);
  };

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Installer Media" description="Upload installer ISOs, manage defaults, and boot them for client OS installation." />

      <Card>
        <CardHeader>
          <CardTitle>Upload an installer ISO</CardTitle>
          <CardDescription>WinPE and Linux live ISOs are structurally checked and checksummed before they enter the catalog.</CardDescription>
        </CardHeader>
        <CardContent>
          <p className="mb-4 text-sm text-muted-foreground">Configured maximum upload size: {maxBytes == null ? "Loading…" : formatBytes(maxBytes)}</p>
          <form className="grid gap-4 md:grid-cols-2" onSubmit={handleUpload}>
            <label className="grid gap-1 text-sm">Display name
              <Input value={displayName} onChange={(event) => setDisplayName(event.target.value)} required maxLength={120} />
            </label>
            <label className="grid gap-1 text-sm">Media type
              <select className="h-9 rounded-lg border border-input bg-background px-3" value={mediaType} onChange={(event) => setMediaType(event.target.value)}>
                {mediaTypes.map((type) => <option key={type.value} value={type.value}>{type.label}</option>)}
              </select>
            </label>
            <label className="grid gap-1 text-sm md:col-span-2">ISO file
              <Input type="file" accept=".iso,application/x-iso9660-image" onChange={(event) => setFile(event.target.files?.[0] || null)} required />
            </label>
            <div className="md:col-span-2">
              <Button disabled={busy || loading || !maxBytes} onClick={handleUpload}>Upload and validate</Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>Installer ISO catalog</CardTitle><CardDescription>Structural validation, checksum, size, and default selection.</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          {loading ? <p role="status">Loading installer media…</p> : media.length === 0 ? <p>No installer ISOs have been uploaded.</p> : media.map((item) => (
            <article key={item.id} className="rounded-lg border p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <h3 className="font-semibold">{item.display_name}</h3>
                  <p className="text-sm text-muted-foreground">{mediaTypes.find((type) => type.value === item.media_type)?.label || item.media_type} · {formatBytes(item.byte_size)} · Validation: {item.validation_state}</p>
                  <p className="break-all text-xs text-muted-foreground">SHA-256: {item.checksum}</p>
                  <p className="mt-2 text-sm">Default: {item.is_default ? "Yes" : "No"}</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  {!item.is_default && <Button variant="outline" disabled={busy} onClick={() => runAction(() => setDefaultInstallerMedia(item.id), `${item.display_name} is now the default for its type.`)}>Set as default</Button>}
                  <Button variant="destructive" disabled={busy} onClick={() => handleDelete(item)}>Delete</Button>
                </div>
              </div>
            </article>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}
