import { useCallback, useEffect, useRef, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button, Input, PageHeader } from "@/components/ui";
import { Progress } from "@/components/ui/progress";
import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { useInstallerMediaUploadStore } from "@/store/useInstallerMediaUploadStore";
import { deleteInstallerMedia, listInstallerMedia, setDefaultInstallerMedia } from "@/api/modules/installerMedia";
import { getSettings } from "@/api/modules/system";

const mediaTypes = [
  { value: "winpe", label: "Windows PE" },
  { value: "windows_iso", label: "Windows ISO" },
  { value: "linux_iso", label: "Linux ISO" },
  { value: "other_iso", label: "Other ISO" },
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
  const [mediaType, setMediaType] = useState("windows_iso");
  const [file, setFile] = useState(null);
  const [fileInputKey, setFileInputKey] = useState(0);
  const upload = useInstallerMediaUploadStore();
  const uploadActive = upload.status === "uploading" || upload.status === "validating";
  const mounted = useRef(true);

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

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

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

  const handleUpload = (event) => {
    event.preventDefault();
    if (uploadActive) return;
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
    const pending = upload.start({ displayName: displayName.trim(), mediaType, file });
    if (!pending) return;
    void pending.then((succeeded) => {
      if (!succeeded || !mounted.current) return;
      setDisplayName("");
      setFile(null);
      setFileInputKey((key) => key + 1);
      void load();
    });
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
          <CardDescription>Installer ISOs are structurally checked and checksummed before they enter the catalog.</CardDescription>
        </CardHeader>
        <CardContent>
          <p className="mb-4 text-sm text-muted-foreground">Configured maximum upload size: {maxBytes == null ? "Loading…" : formatBytes(maxBytes)}</p>
          <form className="grid gap-4 md:grid-cols-2" onSubmit={handleUpload}>
            <label className="grid gap-1 text-sm">Display name
              <Input value={displayName} onChange={(event) => setDisplayName(event.target.value)} required maxLength={120} disabled={uploadActive} />
            </label>
            <label className="grid gap-1 text-sm">Media type
              <select className="h-9 rounded-lg border border-input bg-background px-3" value={mediaType} onChange={(event) => setMediaType(event.target.value)} disabled={uploadActive}>
                {mediaTypes.map((type) => <option key={type.value} value={type.value}>{type.label}</option>)}
              </select>
            </label>
            <label className="grid gap-1 text-sm md:col-span-2">ISO file
              <Input key={fileInputKey} type="file" accept=".iso,application/x-iso9660-image" onChange={(event) => setFile(event.target.files?.[0] || null)} required disabled={uploadActive} />
            </label>
            {upload.status !== "idle" && (
              <div className="grid gap-2 rounded-lg border p-3 md:col-span-2" aria-live="polite">
                {uploadActive ? (
                  <>
                    <div className="flex flex-wrap items-baseline justify-between gap-2 text-sm">
                      <span>
                        {upload.status === "validating"
                          ? `Validating ${upload.fileName}…`
                          : `Uploading ${upload.fileName}…`}
                      </span>
                      <span className="tabular-nums text-muted-foreground">{upload.progress}%</span>
                    </div>
                    <Progress value={upload.progress} aria-label="Installer ISO upload progress" />
                    <p className="text-xs text-muted-foreground">You can leave this page. The upload continues in the background and you will be notified when it finishes.</p>
                  </>
                ) : (
                  <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                    <span className={upload.status === "error" ? "text-destructive" : undefined}>
                      {upload.status === "success"
                        ? `${upload.displayName} uploaded and validated.`
                        : upload.error}
                    </span>
                    <Button variant="ghost" size="sm" onClick={upload.reset}>Dismiss</Button>
                  </div>
                )}
              </div>
            )}
            <div className="md:col-span-2">
              <Button disabled={uploadActive || busy || loading || !maxBytes} onClick={handleUpload}>Upload and validate</Button>
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
                  {!item.is_default && <Button variant="outline" disabled={busy} onClick={() => runAction(() => setDefaultInstallerMedia(item.id), `${item.display_name} is now the default installer ISO.`)}>Set as default</Button>}
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
