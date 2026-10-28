import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Input, PageHeader } from "@/components/ui";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import {
  listInstallations, startInstallation, cancelInstallation, closeInstallationRecovery, retryInstallation,
  getHardwareIdentity, captureHardwareIdentity, applyHardwareIdentity,
  removeHardwareIdentity, requestHardwareInventory,
} from "@/api/modules/clientProvisioning";
import { listInstallerMedia } from "@/api/modules/installerMedia";

const selectClass = "h-9 rounded-lg border border-input bg-background px-3";
const recipes = [
  ["interactive", "Interactive ISO boot"], ["windows", "Windows installation"],
  ["ubuntu", "Ubuntu unattended installation"], ["rhel", "RHEL / compatible unattended installation"],
];
const factoryPhases = ["Prepare", "Install", "Adapt", "Save", "Verify", "Publish"];
const factoryPhaseIndex = { preparing: 0, waiting: 1, installing: 1, adapting: 2, saving: 3, verifying: 4, published: 5 };

export default function ClientProvisioning() {
  const clients = useAppStore((state) => state.clients);
  const fetchClients = useAppStore((state) => state.fetchClients);
  const { success, error: toastError } = useToastStore();
  const confirm = useConfirm();
  const [clientId, setClientId] = useState("");
  const [recipe, setRecipe] = useState("interactive");
  const [username, setUsername] = useState("");
  const [passwordHash, setPasswordHash] = useState("");
  const [media, setMedia] = useState([]);
  const [mediaId, setMediaId] = useState("");
  const [imageName, setImageName] = useState("");
  const [sizeGb, setSizeGb] = useState("");
  const [assignOnReady, setAssignOnReady] = useState(false);
  const [sessions, setSessions] = useState([]);
  const [identity, setIdentity] = useState(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const selectedClient = useRef(clientId);
  const client = clients.find((item) => item.id === clientId);
  const linux = ["ubuntu", "rhel"].includes(recipe);

  useEffect(() => {
    void fetchClients();
    void listInstallerMedia().then((items) => setMedia(items.filter((item) => item.validation_state === "valid"))).catch(() => setMedia([]));
  }, [fetchClients]);
  useEffect(() => { selectedClient.current = clientId; }, [clientId]);
  const load = useCallback(async (notify = true) => {
    if (!clientId) return;
    const requestedClient = clientId;
    try {
      const [installations, hardware] = await Promise.all([listInstallations(requestedClient), getHardwareIdentity(requestedClient)]);
      if (selectedClient.current !== requestedClient) return;
      setSessions(installations);
      setIdentity(hardware);
    } catch (err) {
      if (notify && selectedClient.current === requestedClient) toastError("Client provisioning", err.message || "Could not load client operations.");
    } finally { if (selectedClient.current === requestedClient) setLoading(false); }
  }, [clientId, toastError]);
  useEffect(() => {
    if (!clientId) return;
    const initial = setTimeout(() => { void load(); }, 0);
    const timer = setInterval(() => { void load(false); }, 5000);
    return () => { clearTimeout(initial); clearInterval(timer); };
  }, [clientId, load]);

  const run = async (action, message, question) => {
    if (question && !await confirm({ title: question.title, description: question.description, confirmText: "Continue", confirmVariant: question.destructive ? "destructive" : "default" })) return;
    setBusy(true);
    try { await action(); success("Client provisioning", message); await load(); }
    catch (err) { toastError("Client provisioning", err.message || "The operation failed."); }
    finally { setBusy(false); }
  };
  const chooseClient = (id) => {
    selectedClient.current = id;
    setClientId(id); setIdentity(null); setSessions([]); setPasswordHash(""); setImageName(""); setAssignOnReady(false); setLoading(!!id);
  };
  const start = () => run(async () => {
    await startInstallation(clientId, { recipe, media_id: mediaId, ...(recipe !== "interactive" ? { image_name: imageName } : {}), ...(sizeGb ? { size_gb: Number(sizeGb) } : {}), assign_on_ready: recipe !== "interactive" && assignOnReady, confirm_erase: true, ...(linux ? { username, password_hash: passwordHash } : {}) });
    setPasswordHash("");
  }, "Installation prepared. Boot the selected client to continue.", {
    title: "Prepare client installation", destructive: true,
    description: `${client?.name} will install to a new private draft image. Its current assignment and storage remain untouched. The draft is published only after a successful normal boot verification.`,
  });

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Verified Image Factory" description="Build a private image, adapt it for diskless boot, and publish it only after a successful verification boot." />
      <Card>
        <CardHeader><CardTitle>Client</CardTitle><CardDescription>Select the offline client to prepare.</CardDescription></CardHeader>
        <CardContent className="grid gap-3">
          <label className="grid gap-1 text-sm">Client<select className={selectClass} value={clientId} onChange={(event) => chooseClient(event.target.value)} disabled={busy}>
            <option value="">Select a client</option>
            {clients.map((item) => <option key={item.id} value={item.id}>{item.name} ({item.ip || item.mac})</option>)}
          </select></label>
          {client && <p className="break-all text-sm text-muted-foreground">Assigned image: {client.master || "None"} · Firmware: {client.pxe_mode} · Status: {client.status}</p>}
          {loading && <p role="status">Loading client operations…</p>}
        </CardContent>
      </Card>
      {client && <>
        <Card>
          <CardHeader><CardTitle>New image</CardTitle><CardDescription>The client’s assigned disk, writeback, and game storage are not used. Unattended recipes require compatible full installation media and UEFI.</CardDescription></CardHeader>
          <CardContent className="grid gap-4">
            <label className="grid gap-1 text-sm">Installer media<select className={selectClass} value={mediaId} onChange={(event) => setMediaId(event.target.value)} disabled={busy}><option value="">Select validated media</option>{media.map((item) => <option key={item.id} value={item.id}>{item.display_name} ({item.media_type})</option>)}</select></label>
            <label className="grid gap-1 text-sm">Installation method<select className={selectClass} value={recipe} onChange={(event) => setRecipe(event.target.value)} disabled={busy}>
              {recipes.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
            </select></label>
            {(recipe !== "interactive" || !client.master || client.master === "pending") && <div className="grid gap-3 sm:grid-cols-2">{recipe !== "interactive" && <label className="grid gap-1 text-sm">Published image name<Input value={imageName} onChange={(event) => setImageName(event.target.value)} placeholder="ubuntu-workstations" /></label>}<label className="grid gap-1 text-sm">Size in GiB<Input type="number" min="8" value={sizeGb} onChange={(event) => setSizeGb(event.target.value)} placeholder={client.master && client.master !== "pending" ? "Use assigned image size" : "At least 8"} /></label></div>}
            {linux && <div className="grid gap-3 sm:grid-cols-2">
              <label className="grid gap-1 text-sm">Linux username<Input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off" /></label>
              <label className="grid gap-1 text-sm">SHA-512 password hash<Input type="password" value={passwordHash} onChange={(event) => setPasswordHash(event.target.value)} autoComplete="new-password" placeholder="$6$…" /></label>
            </div>}
            {recipe !== "interactive" && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={assignOnReady} onChange={(event) => setAssignOnReady(event.target.checked)} />Assign to this client after verification</label>}
            <div><Button disabled={busy || loading || !mediaId || (recipe !== "interactive" && !imageName) || ((!client.master || client.master === "pending") && !sizeGb) || (linux && (!username || !passwordHash))} onClick={start}>{recipe === "interactive" ? "Start manual boot" : "Create verified image"}</Button></div>
            {sessions.map(({ session, events, actions = [] }) => <article key={session.id} className="grid gap-2 rounded-lg border p-3">
              <p className="text-sm font-medium">{recipes.find(([value]) => value === session.recipe)?.[1] || session.recipe}: {session.state} · {session.progress}%</p>
              {session.kind === "factory" && factoryPhaseIndex[session.state] !== undefined && <ol aria-label="Image factory phase" className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
                {factoryPhases.map((phase, index) => <li key={phase} aria-current={index === factoryPhaseIndex[session.state] ? "step" : undefined} className={index <= factoryPhaseIndex[session.state] ? "font-medium" : "text-muted-foreground"}>{index + 1}. {phase}</li>)}
              </ol>}
              {session.kind === "factory" && <p className="break-all text-xs text-muted-foreground">Image: {session.image_name || "preparing"} · Verification attempt: {session.attempt} · Cleanup: {session.cleanup_state}</p>}
              {session.phase_deadline && !session.closed_at && <p className="text-xs text-muted-foreground">Phase deadline: {new Date(session.phase_deadline).toLocaleString()}</p>}
              {["saving", "verifying"].includes(session.state) && client?.status?.toLowerCase() !== "online" && <p role="status" className="text-xs text-muted-foreground">Power on {client.name} so it can PXE boot and report the verification result.</p>}
              {session.published_image_id && <p className="break-all text-sm">Published image: {session.image_name || session.published_image_id} · Assignment: {session.assignment_state}{session.assignment_error ? ` — ${session.assignment_error}` : ""}</p>}
              {session.error && <p className="text-sm text-destructive">{session.error}</p>}
              <div className="flex flex-wrap gap-2">
                {actions.includes("retry") && <Button variant="outline" disabled={busy} onClick={() => run(() => retryInstallation(session.id), "Installation retry started.", { title: "Retry installation", description: "The server checks that the client is off and has no active session before starting the retry." })}>Retry</Button>}
                {actions.includes("cancel") && <Button variant="outline" disabled={busy} onClick={() => run(() => cancelInstallation(session.id), "New installer requests stopped. Shut down the client before clearing recovery.", { title: "Cancel installation session", description: "An installer already running can continue writing. Shut down the client and clear recovery only after it has stopped.", destructive: true })}>Cancel session</Button>}
                {actions.includes("close") && <Button variant="outline" disabled={busy} onClick={() => run(() => closeInstallationRecovery(session.id), "Installation recovery closed.", { title: "Close installation recovery", description: "Confirm the client is shut down and its installer has stopped. The server checks for active storage sessions before releasing this operation." })}>Clear recovery</Button>}
                {actions.includes("retry_cleanup") && <Button variant="outline" disabled={busy} onClick={() => run(() => closeInstallationRecovery(session.id), "Cleanup retry started.", { title: "Retry cleanup", description: "The server will retry cleanup only when the client and session targets are offline." })}>Retry cleanup</Button>}
              </div>
              {events?.length > 0 && <details><summary className="cursor-pointer text-sm">Installation events</summary><ol className="mt-2 space-y-1 text-xs text-muted-foreground">{events.map((event, index) => <li key={event.id || index}>{event.created_at}: {event.message || event.state}</li>)}</ol></details>}
            </article>)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>Hardware identity</CardTitle><CardDescription>Capture a working Windows client after a clean shutdown. Apply its saved identity only to the same client, hardware profile, and source revision.</CardDescription></CardHeader>
          <CardContent className="grid gap-3">
            {identity?.profile ? <p className="break-all text-sm">Observed profile: {identity.profile.architecture} · {identity.profile.firmware}<br />Boot NIC IDs: {identity.profile.boot_nic_ids}</p> : <p className="text-sm text-muted-foreground">Collect a WinPE hardware profile before capturing this client.</p>}
            <div className="flex flex-wrap gap-2">
              <Button variant="outline" disabled={busy} onClick={() => run(() => requestHardwareInventory(clientId), "Hardware inventory requested. Reboot the client into WinPE.")}>Collect hardware profile</Button>
              <Button disabled={busy || !identity?.profile} onClick={() => run(() => captureHardwareIdentity(clientId), "Client identity captured.", { title: "Capture Windows client identity", description: "Confirm Windows was working on this client and has shut down cleanly. The server checks exclusive ownership and retains an immutable baseline." })}>Capture identity</Button>
              <Button variant="outline" disabled={busy || !identity?.cache || identity.cache.state !== "ready"} onClick={() => run(() => applyHardwareIdentity(clientId), "Saved identity applied.", { title: "Apply saved client identity", description: "The client's current disk is retained for recovery. A replacement disk is created from this client's saved identity.", destructive: true })}>Apply identity</Button>
              <Button variant="destructive" disabled={busy || !identity?.cache} onClick={() => run(() => removeHardwareIdentity(clientId), "Original client disk restored and identity cache removed.", { title: "Remove saved identity", description: "Restore the original client disk and remove the identity cache. The client must be shut down.", destructive: true })}>Remove identity</Button>
            </div>
            {identity?.cache && <div className="text-sm"><p>Cache: {identity.cache.state}</p><p className="break-all text-muted-foreground">Source: {identity.cache.source_revision}</p>{identity.cache.last_error && <p className="text-destructive">{identity.cache.last_error}</p>}</div>}
          </CardContent>
        </Card>
      </>}
    </div>
  );
}
