import { useCallback, useEffect, useMemo, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button, Input, PageHeader } from "@/components/ui";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { getSuperStatus } from "@/api/modules/clients";
import { deleteInstallerMedia, listInstallerMedia, recordInstallerMediaVerification, setDefaultInstallerMedia, uploadInstallerMedia } from "@/api/modules/installerMedia";
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

function revisionFor(client) {
  return client?.snapshot || client?.master || "";
}

export default function InstallerMedia() {
  const clients = useAppStore((state) => state.clients);
  const fetchClients = useAppStore((state) => state.fetchClients);
  const { success, error: toastError } = useToastStore();
  const confirm = useConfirm();
  const [media, setMedia] = useState([]);
  const [maxBytes, setMaxBytes] = useState(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [mediaType, setMediaType] = useState("winpe");
  const [file, setFile] = useState(null);
  const [clientId, setClientId] = useState("");
  const [verificationStatus, setVerificationStatus] = useState(null);
  const [bootResult, setBootResult] = useState("passed");
  const [observedLuns, setObservedLuns] = useState([0, 1]);

  const selectedClient = useMemo(
    () => clients.find((client) => client.id === clientId),
    [clients, clientId],
  );

  useEffect(() => {
    if (!clientId) return;

    let active = true;
    getSuperStatus(clientId)
      .then((status) => { if (active) setVerificationStatus({ clientId, status }); })
      .catch(() => { if (active) setVerificationStatus({ clientId, error: true }); });
    return () => { active = false; };
  }, [clientId]);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [catalog, settings] = await Promise.all([listInstallerMedia(), getSettings(), fetchClients()]);
      setMedia(catalog);
      setMaxBytes(settings?.installer_media?.max_upload_bytes ?? null);
    } catch (err) {
      setError(err.message || "Could not load installer media.");
    } finally {
      setLoading(false);
    }
  }, [fetchClients]);

  useEffect(() => {
    const timer = setTimeout(() => { void load(); }, 0);
    return () => clearTimeout(timer);
  }, [load]);

  const runAction = async (action, successMessage) => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
      success("Installer media", successMessage);
      setNotice(successMessage);
      await load();
    } catch (err) {
      setError(err.message || "The installer media operation failed.");
      toastError("Installer media", err.message || "The operation failed.");
    } finally {
      setBusy(false);
    }
  };

  const handleUpload = async (event) => {
    event.preventDefault();
    if (!file || !displayName.trim()) {
      setError("Enter a display name and choose an ISO file.");
      return;
    }
    if (!file.name.toLowerCase().endsWith(".iso")) {
      setError("Choose a file with the .iso extension.");
      return;
    }
    if (maxBytes != null && file.size > maxBytes) {
      setError(`This file exceeds the configured limit of ${formatBytes(maxBytes)}.`);
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

  const handleVerification = (item) => runAction(
    () => recordInstallerMediaVerification(item.id, {
      checksum: item.checksum,
      firmware_mode: selectedClient.pxe_mode,
      test_client_id: selectedClient.id,
      assigned_image_revision: revisionFor(selectedClient),
      observed_luns: observedLuns,
      boot_result: bootResult,
    }),
    `${item.display_name} ${selectedClient.pxe_mode.toUpperCase()} boot result recorded.`,
  );

  const selectedVerificationStatus = verificationStatus?.clientId === clientId ? verificationStatus : null;
  const verificationStatusLoading = !!clientId && !selectedVerificationStatus;
  const verificationSession = selectedVerificationStatus?.status?.session;
  const hasActiveVerificationSession = verificationSession?.state?.toLowerCase() === "active" && verificationSession.verification_only;
  const sessionMedia = media.find((item) => item.id === verificationSession?.installer_media_id);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Installer Media" description="Upload and validate installer ISOs, manage defaults, and record hardware boot checks." />

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
        <CardHeader><CardTitle>Installer ISO catalog</CardTitle><CardDescription>Structural validation, checksum, size, default selection, and BIOS/UEFI verification.</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          {loading ? <p role="status">Loading installer media…</p> : error && media.length === 0 ? <p role="alert">{error}</p> : media.length === 0 ? <p>No installer ISOs have been uploaded.</p> : media.map((item) => (
            <article key={item.id} className="rounded-lg border p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <h3 className="font-semibold">{item.display_name}</h3>
                  <p className="text-sm text-muted-foreground">{mediaTypes.find((type) => type.value === item.media_type)?.label || item.media_type} · {formatBytes(item.byte_size)} · Validation: {item.validation_state}</p>
                  <p className="break-all text-xs text-muted-foreground">SHA-256: {item.checksum}</p>
                  <p className="mt-2 text-sm">Default: {item.is_default ? "Yes" : "No"} · BIOS: {item.bios_verified ? "Verified" : "Not verified"} · UEFI: {item.uefi_verified ? "Verified" : "Not verified"}</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  {!item.is_default && <Button variant="outline" disabled={busy} onClick={() => runAction(() => setDefaultInstallerMedia(item.id), `${item.display_name} is now the default for its type.`)}>Set as default</Button>}
                  <Button variant="destructive" disabled={busy} onClick={() => handleDelete(item)}>Delete</Button>
                </div>
              </div>
              <div className="mt-4 grid gap-3 border-t pt-4 md:grid-cols-2">
                <label className="grid gap-1 text-sm">Test client
                  <select className="h-9 rounded-lg border border-input bg-background px-3" value={clientId} onChange={(event) => setClientId(event.target.value)}>
                    <option value="">Select a client</option>
                    {clients.map((client) => <option key={client.id} value={client.id}>{client.name} ({client.id})</option>)}
                  </select>
                </label>
                <div className="text-sm"><span className="font-medium">Assigned revision:</span> {revisionFor(selectedClient) || "Select a test client"}<br /><span className="font-medium">Firmware:</span> {selectedClient?.pxe_mode || "—"}</div>
                <fieldset className="flex flex-wrap items-center gap-4 text-sm">
                  <legend className="mb-1 font-medium">Observed LUNs</legend>
                  {[0, 1].map((lun) => <label key={lun} className="flex items-center gap-2"><input type="checkbox" checked={observedLuns.includes(lun)} onChange={(event) => setObservedLuns((current) => event.target.checked ? [...current, lun].sort() : current.filter((value) => value !== lun))} /> LUN {lun}</label>)}
                </fieldset>
                <label className="grid gap-1 text-sm">Boot result
                  <select className="h-9 rounded-lg border border-input bg-background px-3" value={bootResult} onChange={(event) => setBootResult(event.target.value)}>
                    <option value="passed">Passed</option><option value="failed">Failed</option>
                  </select>
                </label>
                <div className="md:col-span-2">
                  <Button
                    variant="outline"
                    disabled={busy || verificationStatusLoading || !selectedClient || !revisionFor(selectedClient) || !["bios", "uefi"].includes(selectedClient?.pxe_mode) || !hasActiveVerificationSession || verificationSession.installer_media_id !== item.id}
                    onClick={() => handleVerification(item)}
                  >Record observed boot result</Button>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {!selectedClient ? "Select a client to check its verification session." : verificationStatusLoading ? "Checking the selected client’s verification session…" : selectedVerificationStatus?.error ? "Could not check the client’s verification session; reload the client selection and try again." : !hasActiveVerificationSession ? "Start an active verification-only session from Client Management before recording a result." : verificationSession.installer_media_id !== item.id ? `The active verification session is using a different ISO${sessionMedia ? ` (${sessionMedia.display_name})` : ""}.` : "This result will be recorded for the ISO used by the active verification-only session."}
                  </p>
                </div>
              </div>
            </article>
          ))}
          {!loading && error && media.length > 0 && <p role="alert">{error}</p>}
          {notice && <p role="status">{notice}</p>}
        </CardContent>
      </Card>
    </div>
  );
}
