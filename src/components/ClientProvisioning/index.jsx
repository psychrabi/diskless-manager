import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Input, PageHeader } from "@/components/ui";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import {
  listInstallations, startInstallation, cancelInstallation, closeInstallationRecovery, retryInstallation,
  getHardwareIdentity, captureHardwareIdentity, applyHardwareIdentity,
  removeHardwareIdentity, requestHardwareInventory,
} from "@/api/modules/clientProvisioning";
import { listMasters } from "@/api/modules/images";
import { listInstallerMedia } from "@/api/modules/installerMedia";
import { ArrowRight, CheckCircle2, Disc3, HardDrive, MonitorCheck } from "lucide-react";
import { cn } from "@/lib/utils";

const selectClass = "h-10 w-full rounded-lg border border-input bg-background px-3 text-[13px] text-foreground shadow-xs outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/20 disabled:cursor-not-allowed disabled:opacity-60";
const recipes = [
  ["interactive", "Interactive ISO boot"], ["windows", "Windows installation"],
  ["windows_adapt", "Adapt existing Windows image"],
  ["ubuntu", "Ubuntu unattended installation"], ["rhel", "RHEL / compatible unattended installation"],
];
const factoryPhases = ["Prepare", "Install", "Adapt", "Save", "Verify", "Publish"];
const factoryPhaseIndex = { preparing: 0, waiting: 1, installing: 1, adapting: 2, saving: 3, verifying: 4, published: 5 };

const IntroCard = ({ icon: Icon, title, children }) => (
  <div className="rounded-2xl border border-dashed border-border/80 bg-card p-5 shadow-none">
    <div className="flex items-center">
      <span className="flex size-9 items-center justify-center rounded-xl border border-primary/15 bg-primary/10 text-primary">
        <Icon className="size-4" aria-hidden="true" />
      </span>
    </div>
    <h3 className="mt-4 text-[13px] font-semibold text-foreground">{title}</h3>
    <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{children}</p>
  </div>
);

export default function ClientProvisioning() {
  const clients = useAppStore((state) => state.clients);
  const fetchClients = useAppStore((state) => state.fetchClients);
  const { success, error: toastError } = useToastStore();
  const confirm = useConfirm();
  const [clientId, setClientId] = useState("");
  const [recipe, setRecipe] = useState("interactive");
  const [username, setUsername] = useState("");
  const [passwordHash, setPasswordHash] = useState("");
  const [images, setImages] = useState([]);
  const [sourceImageId, setSourceImageId] = useState("");
  const [sourceSnapshot, setSourceSnapshot] = useState("");
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
  const adapting = recipe === "windows_adapt";
  const sourceImage = images.find((image) => image.id === sourceImageId);
  const linux = ["ubuntu", "rhel"].includes(recipe);

  useEffect(() => {
    void fetchClients();
    void listMasters().then((items) => setImages(items.filter((item) => item.os_type === "windows" && item.status === "ready"))).catch(() => { setImages([]); toastError("Source images", "Could not load ready Windows images."); });
    void listInstallerMedia().then((items) => setMedia(items.filter((item) => item.validation_state === "valid"))).catch(() => setMedia([]));
  }, [fetchClients, toastError]);
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
    await startInstallation(clientId, { recipe: adapting ? "windows" : recipe, media_id: mediaId, ...(adapting ? { source_image_id: sourceImageId, source_snapshot: sourceSnapshot } : {}), ...(recipe !== "interactive" ? { image_name: imageName } : {}), ...(!adapting && sizeGb ? { size_gb: Number(sizeGb) } : {}), assign_on_ready: recipe !== "interactive" && assignOnReady, confirm_erase: true, ...(linux ? { username, password_hash: passwordHash } : {}) });
    setPasswordHash("");
  }, "Installation prepared. Boot the selected client to continue.", {
    title: "Prepare client installation", destructive: true,
    description: adapting ? `${client?.name} will adapt only a private clone of the selected Windows source snapshot. The source image and current assignment remain untouched. The new image is published only after verification, and assigned only if you explicitly opt in.` : `${client?.name} will install to a new private draft image. Its current assignment and storage remain untouched. The draft is published only after a successful normal boot verification.`,
  });

  const canStart = !busy && !loading && mediaId && (recipe === "interactive" || imageName) && (adapting ? sourceImage && sourceSnapshot : recipe === "interactive" || client?.master && client.master !== "pending" ? true : sizeGb) && (!linux || (username && passwordHash));

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Verified Image Factory"
        description="Build a private image, adapt it for diskless boot, and publish it only after a successful verification boot."
      />

      <div className="rounded-2xl border border-border/80 bg-card p-5 shadow-[0_1px_2px_rgb(0_0_0/0.03)]">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-xs font-medium text-muted-foreground">Pipeline</p>
            <p className="mt-1.5 text-[13px] text-muted-foreground">Nothing reaches the fleet until a real client boots the result.</p>
          </div>
          <Badge variant="outline">Offline client required</Badge>
        </div>
        <ol className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
          {factoryPhases.map((phase, index) => (
            <li key={phase} className="flex items-center gap-2 rounded-xl border border-border/60 bg-muted/20 px-3 py-2">
              <span className="flex size-5 shrink-0 items-center justify-center rounded-md border border-border/70 bg-card font-mono-data text-[9px] font-semibold text-muted-foreground">{index + 1}</span>
              <span className="text-[11px] font-medium text-foreground">{phase}</span>
              {index < factoryPhases.length - 1 && <ArrowRight className="ml-auto hidden size-3 text-muted-foreground/40 xl:block" aria-hidden="true" />}
            </li>
          ))}
        </ol>
      </div>

      <div className={cn("grid gap-5", !client && "xl:grid-cols-[minmax(0,1.35fr)_minmax(0,0.65fr)]")}>
        <Card>
          <CardHeader>
            <CardTitle>Target client</CardTitle>
            <CardDescription>Select the offline machine that will build and verify this image.</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3">
            <label htmlFor="factory-client" className="text-xs font-medium text-muted-foreground">Client</label>
            <select id="factory-client" className={cn(selectClass, "max-w-md")} value={clientId} onChange={(event) => chooseClient(event.target.value)} disabled={busy}>
              <option value="">Select a client</option>
              {clients.map((item) => <option key={item.id} value={item.id}>{item.name} ({item.ip || item.mac})</option>)}
            </select>
            {client && (
              <div className="rounded-xl border border-border/70 bg-muted/20 p-3">
                <p className="break-all text-[12px] text-muted-foreground">Assigned image: {client.master || "None"} · Firmware: {client.pxe_mode} · Status: {client.status}</p>
              </div>
            )}
            {loading && <p role="status" className="text-xs text-muted-foreground">Loading client operations…</p>}
          </CardContent>
        </Card>
        {!client && (
          <Card className="bg-muted/15">
            <CardHeader>
              <CardTitle>Before you begin</CardTitle>
              <CardDescription>Three checks keep a factory run predictable.</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              {[
                ["Client is powered off", "The factory verifies the target is idle before it starts."],
                ["Media is validated", "Unverified or incomplete installer media is filtered out."],
                ["Current image is safe", "Builds always happen in a private draft, never in place."],
              ].map(([title, body]) => (
                <div key={title} className="flex gap-2.5">
                  <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-success" aria-hidden="true" />
                  <div>
                    <p className="text-[12px] font-medium text-foreground">{title}</p>
                    <p className="mt-0.5 text-[11px] leading-4 text-muted-foreground">{body}</p>
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>
        )}
      </div>

      {!client ? (
        <div className="grid gap-4 md:grid-cols-3">
          <IntroCard icon={HardDrive} title="Choose an offline target">The factory uses this machine as a temporary builder. Its current disk and assignment stay untouched.</IntroCard>
          <IntroCard icon={Disc3} title="Provide installation media">Use validated Windows or Linux media. Adapting an existing Windows image needs a ready source snapshot.</IntroCard>
          <IntroCard icon={MonitorCheck} title="Prove it boots">The client PXE boots the draft. The image is published only after a normal boot reports back successfully.</IntroCard>
        </div>
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            {[
              ["Assigned image", client.master || "None"],
              ["Firmware", client.pxe_mode || "Unknown"],
              ["Status", client.status || "Unknown"],
            ].map(([label, value]) => (
              <div key={label} className="rounded-2xl border border-border/80 bg-card p-4">
                <p className="text-xs font-medium text-muted-foreground">{label}</p>
                <p className="mt-1.5 truncate font-mono-data text-[12px] font-medium text-foreground">{value}</p>
              </div>
            ))}
          </div>

          <Card>
            <CardHeader>
              <CardTitle>New image</CardTitle>
              <CardDescription>The client’s assigned disk, writeback, and game storage are not used. Unattended recipes require compatible full installation media and UEFI.</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="grid gap-1.5">
                  <label htmlFor="factory-media" className="text-xs font-medium text-muted-foreground">Installer media</label>
                  <select id="factory-media" className={selectClass} value={mediaId} onChange={(event) => setMediaId(event.target.value)} disabled={busy}>
                    <option value="">Select validated media</option>
                    {media.map((item) => <option key={item.id} value={item.id}>{item.display_name} ({item.media_type})</option>)}
                  </select>
                </div>
                <div className="grid gap-1.5">
                  <label htmlFor="factory-method" className="text-xs font-medium text-muted-foreground">Installation method</label>
                  <select id="factory-method" className={selectClass} value={recipe} onChange={(event) => setRecipe(event.target.value)} disabled={busy}>
                    {recipes.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                  </select>
                </div>
              </div>

              {adapting && (
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="grid gap-1.5">
                    <label htmlFor="factory-source-image" className="text-xs font-medium text-muted-foreground">Source Windows image</label>
                    <select id="factory-source-image" className={selectClass} value={sourceImageId} onChange={(event) => { setSourceImageId(event.target.value); setSourceSnapshot(""); }} disabled={busy}>
                      <option value="">Select ready Windows image</option>
                      {images.map((image) => <option key={image.id} value={image.id}>{image.name}</option>)}
                    </select>
                  </div>
                  <div className="grid gap-1.5">
                    <label htmlFor="factory-source-snapshot" className="text-xs font-medium text-muted-foreground">Source snapshot</label>
                    <select id="factory-source-snapshot" className={selectClass} value={sourceSnapshot} onChange={(event) => setSourceSnapshot(event.target.value)} disabled={busy || !sourceImage}>
                      <option value="">Select source snapshot</option>
                      {sourceImage?.snapshots?.map((snapshot) => <option key={snapshot.name} value={snapshot.name}>{snapshot.name}</option>)}
                    </select>
                  </div>
                </div>
              )}

              {(recipe !== "interactive" || !client.master || client.master === "pending") && (
                <div className="grid gap-3 sm:grid-cols-2">
                  {recipe !== "interactive" && (
                    <div className="grid gap-1.5">
                      <label htmlFor="factory-image-name" className="text-xs font-medium text-muted-foreground">Published image name</label>
                      <Input id="factory-image-name" value={imageName} onChange={(event) => setImageName(event.target.value)} placeholder="ubuntu-workstations" className="h-10" />
                    </div>
                  )}
                  <div className="grid gap-1.5">
                    <label htmlFor="factory-size" className="text-xs font-medium text-muted-foreground">Size in GiB</label>
                    <Input id="factory-size" type="number" min="8" disabled={adapting} value={adapting ? "" : sizeGb} onChange={(event) => setSizeGb(event.target.value)} placeholder={adapting ? "Inferred from source snapshot" : client.master && client.master !== "pending" ? "Use assigned image size" : "At least 8"} className="h-10" />
                  </div>
                </div>
              )}

              {linux && (
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="grid gap-1.5">
                    <label htmlFor="factory-username" className="text-xs font-medium text-muted-foreground">Linux username</label>
                    <Input id="factory-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off" className="h-10" />
                  </div>
                  <div className="grid gap-1.5">
                    <label htmlFor="factory-password-hash" className="text-xs font-medium text-muted-foreground">SHA-512 password hash</label>
                    <Input id="factory-password-hash" type="password" value={passwordHash} onChange={(event) => setPasswordHash(event.target.value)} autoComplete="new-password" placeholder="$6$…" className="h-10" />
                  </div>
                </div>
              )}

              {recipe !== "interactive" && (
                <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
                  <input type="checkbox" checked={assignOnReady} onChange={(event) => setAssignOnReady(event.target.checked)} />
                  Assign to this client after verification
                </label>
              )}

              <div className="flex flex-col gap-3 border-t border-border/60 pt-4 sm:flex-row sm:items-center sm:justify-between">
                <p className="text-[11px] leading-5 text-muted-foreground">
                  {adapting ? "Only a private clone is changed. The source snapshot remains untouched." : "A private draft is created first. The current assignment is not replaced."}
                </p>
                <Button variant="primary" className="w-full sm:w-auto" disabled={!canStart} onClick={start}>
                  {recipe === "interactive" ? "Start manual boot" : "Create verified image"}
                </Button>
              </div>

              {sessions.length > 0 && (
                <div className="grid gap-3 border-t border-border/60 pt-4">
                  <p className="text-xs font-medium text-muted-foreground">Installation sessions</p>
                  {sessions.map(({ session, events, actions = [] }) => {
                    const phase = factoryPhaseIndex[session.state];
                    return (
                      <article key={session.id} className="grid gap-3 rounded-xl border border-border/80 bg-muted/10 p-4">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <p className="text-[13px] font-medium text-foreground">
                            {recipes.find(([value]) => value === session.recipe)?.[1] || session.recipe}: {session.state} · {session.progress}%
                          </p>
                          <Badge variant={session.state === "failed" ? "destructive" : "outline"}>
                            {session.kind === "factory" ? "Factory" : "Installer"}
                          </Badge>
                        </div>
                        {session.kind === "factory" && phase !== undefined && (
                          <ol aria-label="Image factory phase" className="flex flex-wrap gap-1.5 text-[10px]">
                            {factoryPhases.map((name, index) => (
                              <li key={name} aria-current={index === phase ? "step" : undefined} className={index <= phase ? "rounded-md border border-primary/15 bg-primary/10 px-2 py-1 font-medium text-primary" : "rounded-md border border-border/60 px-2 py-1 text-muted-foreground"}>{index + 1}. {name}</li>
                            ))}
                          </ol>
                        )}
                        {session.kind === "factory" && <p className="break-all font-mono-data text-[10px] text-muted-foreground">Image: {session.image_name || "preparing"} · Verification attempt: {session.attempt} · Cleanup: {session.cleanup_state}</p>}
                        {session.phase_deadline && !session.closed_at && <p className="text-[10px] text-muted-foreground">Phase deadline: {new Date(session.phase_deadline).toLocaleString()}</p>}
                        {["saving", "verifying"].includes(session.state) && client?.status?.toLowerCase() !== "online" && <p role="status" className="rounded-lg border border-info/20 bg-info/5 p-2.5 text-[11px] text-info">Power on {client.name} so it can PXE boot and report the verification result.</p>}
                        {session.published_image_id && <p className="break-all text-[12px] text-foreground">Published image: {session.image_name || session.published_image_id} · Assignment: {session.assignment_state}{session.assignment_error ? ` — ${session.assignment_error}` : ""}</p>}
                        {session.error && <p className="text-[12px] text-destructive">{session.error}</p>}
                        <div className="flex flex-wrap gap-2">
                          {actions.includes("retry") && <Button variant="outline" size="sm" disabled={busy} onClick={() => run(() => retryInstallation(session.id), "Installation retry started.", { title: "Retry installation", description: "The server checks that the client is off and has no active session before starting the retry." })}>Retry</Button>}
                          {actions.includes("cancel") && <Button variant="outline" size="sm" disabled={busy} onClick={() => run(() => cancelInstallation(session.id), "New installer requests stopped. Shut down the client before clearing recovery.", { title: "Cancel installation session", description: "An installer already running can continue writing. Shut down the client and clear recovery only after it has stopped.", destructive: true })}>Cancel session</Button>}
                          {actions.includes("close") && <Button variant="outline" size="sm" disabled={busy} onClick={() => run(() => closeInstallationRecovery(session.id), "Installation recovery closed.", { title: "Close installation recovery", description: "Confirm the client is shut down and its installer has stopped. The server checks for active storage sessions before releasing this operation." })}>Clear recovery</Button>}
                          {actions.includes("retry_cleanup") && <Button variant="outline" size="sm" disabled={busy} onClick={() => run(() => closeInstallationRecovery(session.id), "Cleanup retry started.", { title: "Retry cleanup", description: "The server will retry cleanup only when the client and session targets are offline." })}>Retry cleanup</Button>}
                        </div>
                        {events?.length > 0 && <details><summary className="cursor-pointer text-[11px] text-muted-foreground">Installation events</summary><ol className="mt-2 space-y-1 font-mono-data text-[10px] text-muted-foreground">{events.map((event, index) => <li key={event.id || index}>{event.created_at}: {event.message || event.state}</li>)}</ol></details>}
                      </article>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Hardware identity</CardTitle>
              <CardDescription>Capture a working Windows client after a clean shutdown. Apply its saved identity only to the same client, hardware profile, and source revision.</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              {identity?.profile ? <p className="break-all font-mono-data text-[11px] text-muted-foreground">Observed profile: {identity.profile.architecture} · {identity.profile.firmware}<br />Boot NIC IDs: {identity.profile.boot_nic_ids}</p> : <p className="text-[12px] text-muted-foreground">Collect a WinPE hardware profile before capturing this client.</p>}
              <div className="flex flex-wrap gap-2">
                <Button variant="outline" size="sm" disabled={busy} onClick={() => run(() => requestHardwareInventory(clientId), "Hardware inventory requested. Reboot the client into WinPE.")}>Collect hardware profile</Button>
                <Button variant="outline" size="sm" disabled={busy || !identity?.profile} onClick={() => run(() => captureHardwareIdentity(clientId), "Client identity captured.", { title: "Capture Windows client identity", description: "Confirm Windows was working on this client and has shut down cleanly. The server checks exclusive ownership and retains an immutable baseline." })}>Capture identity</Button>
                <Button variant="outline" size="sm" disabled={busy || !identity?.cache || identity.cache.state !== "ready"} onClick={() => run(() => applyHardwareIdentity(clientId), "Saved identity applied.", { title: "Apply saved client identity", description: "The client's current disk is retained for recovery. A replacement disk is created from this client's saved identity.", destructive: true })}>Apply identity</Button>
                <Button variant="outline" size="sm" disabled={busy || !identity?.cache} onClick={() => run(() => removeHardwareIdentity(clientId), "Original client disk restored and identity cache removed.", { title: "Remove saved identity", description: "Restore the original client disk and remove the identity cache. The client must be shut down.", destructive: true })}>Remove identity</Button>
              </div>
              {identity?.cache && <div className="rounded-xl border border-border/70 bg-muted/15 p-3 text-[11px]"><p>Cache: {identity.cache.state}</p><p className="break-all font-mono-data text-muted-foreground">Source: {identity.cache.source_revision}</p>{identity.cache.last_error && <p className="text-destructive">{identity.cache.last_error}</p>}</div>}
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
