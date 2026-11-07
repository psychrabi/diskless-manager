import { useEffect, useMemo, useState } from "react";
import { listInstallerMedia } from "@/api/modules/installerMedia";
import { buildLinuxImage, preflightLinuxImageBuild } from "@/api/modules/linuxImageBuilder";
import { Button, Modal } from "@/components/ui";
import { DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { useToastStore } from "@/store/useToastStore";

export default function LinuxImageBuilderModal({ isOpen, onClose, onChanged }) {
  const [media, setMedia] = useState([]);
  const [mediaLoading, setMediaLoading] = useState(true);
  const [mediaId, setMediaId] = useState("");
  const [name, setName] = useState("");
  const [sizeGb, setSizeGb] = useState("50");
  const [username, setUsername] = useState("diskless");
  const [password, setPassword] = useState("");
  const [sudoUser, setSudoUser] = useState(true);
  const [preflight, setPreflight] = useState(null);
  const [confirmed, setConfirmed] = useState(false);
  const [checking, setChecking] = useState(false);
  const [building, setBuilding] = useState(false);
  const [operationError, setOperationError] = useState("");
  const { success, error } = useToastStore();

  const linuxIsos = useMemo(
    () => media.filter((item) => item.media_type === "linux_iso" && item.validation_state === "valid"),
    [media],
  );
  const numericSize = Number(sizeGb);
  const canCheck = !mediaLoading && !checking && !building && mediaId && Number.isInteger(numericSize) && numericSize >= 20;
  const canBuild = canCheck && preflight && !building && name.trim() && username.trim() && password && confirmed;

  useEffect(() => {
    if (!isOpen) return undefined;
    let active = true;
    listInstallerMedia()
      .then((items) => {
        if (!active) return;
        const isos = Array.isArray(items) ? items : [];
        setMedia(isos);
        const firstIso = isos.find((item) => item.media_type === "linux_iso" && item.validation_state === "valid");
        setMediaId((current) => current || firstIso?.id || "");
      })
      .catch((cause) => {
        if (active) error("Linux Image Builder", cause?.message || String(cause));
      })
      .finally(() => {
        if (active) setMediaLoading(false);
      });
    return () => { active = false; };
  }, [isOpen, error]);

  const invalidatePreflight = () => {
    setPreflight(null);
    setConfirmed(false);
    setOperationError("");
  };

  const checkIso = async () => {
    setChecking(true);
    setOperationError("");
    invalidatePreflight();
    try {
      const result = await preflightLinuxImageBuild({ media_id: mediaId, size_gb: numericSize });
      setPreflight(result);
    } catch (cause) {
      setOperationError(cause?.message || String(cause));
    } finally {
      setChecking(false);
    }
  };

  const build = async () => {
    setBuilding(true);
    setOperationError("");
    try {
      const image = await buildLinuxImage({
        media_id: mediaId,
        name: name.trim(),
        size_gb: numericSize,
        username: username.trim(),
        password,
        sudo_user: sudoUser,
        confirm_create: true,
      });
      success("Linux Image Builder", `${image.name} is ready for iSCSI boot.`);
      setPassword("");
      setMediaLoading(true);
      await onChanged?.();
      onClose?.();
    } catch (cause) {
      setOperationError(cause?.message || String(cause));
      error("Linux Image Builder", cause?.message || String(cause));
      await onChanged?.();
    } finally {
      setBuilding(false);
    }
  };

  const close = () => {
    if (checking || building) return;
    setPassword("");
    setMediaLoading(true);
    onClose?.();
  };

  return (
    <Modal isOpen={isOpen} onClose={close} title="Build Linux iSCSI Image" size="lg">
      <div className="space-y-4">
        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="linux-builder-iso">Linux ISO</label>
          <NativeSelect
            id="linux-builder-iso"
            value={mediaId}
            disabled={mediaLoading || checking || building || linuxIsos.length === 0}
            onChange={(event) => { setMediaId(event.target.value); invalidatePreflight(); }}
          >
            <NativeSelectOption value="">{mediaLoading ? "Loading installer media…" : "Select an uploaded Linux ISO"}</NativeSelectOption>
            {linuxIsos.map((item) => <NativeSelectOption key={item.id} value={item.id}>{item.display_name}</NativeSelectOption>)}
          </NativeSelect>
          {linuxIsos.length === 0 && !mediaLoading && (
            <p className="text-xs text-muted-foreground">Upload a structurally valid Linux ISO under Installer Media first.</p>
          )}
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="linux-builder-name">Image name</label>
            <Input
              id="linux-builder-name"
              value={name}
              onChange={(event) => { setName(event.target.value); setConfirmed(false); }}
              placeholder="ubuntu-workstations"
              disabled={checking || building}
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="linux-builder-size">Size (GiB)</label>
            <Input
              id="linux-builder-size"
              type="number"
              min="20"
              step="1"
              value={sizeGb}
              onChange={(event) => { setSizeGb(event.target.value); invalidatePreflight(); }}
              disabled={checking || building}
            />
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="linux-builder-username">First-boot username</label>
            <Input
              id="linux-builder-username"
              autoComplete="username"
              value={username}
              onChange={(event) => { setUsername(event.target.value); setConfirmed(false); }}
              placeholder="diskless"
              disabled={checking || building}
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="linux-builder-password">First-boot password</label>
            <Input
              id="linux-builder-password"
              type="password"
              autoComplete="new-password"
              value={password}
              onChange={(event) => { setPassword(event.target.value); setConfirmed(false); }}
              disabled={checking || building}
            />
          </div>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="accent-primary"
            checked={sudoUser}
            disabled={checking || building}
            onChange={(event) => { setSudoUser(event.target.checked); setConfirmed(false); }}
          />
          Grant this account sudo access
        </label>

        <div className="rounded-lg border border-border bg-muted/30 p-3 text-sm text-muted-foreground">
          The builder supports Ubuntu/Kubuntu, Pop!_OS, Fedora/Nobara, and CachyOS live ISOs. Secure Boot must be disabled on clients.
          The resulting image uses the client's iBFT data, so it does not store a client NIC or iSCSI identity.
        </div>

        {preflight && (
          <p role="status" className="rounded-lg border border-success/30 bg-success/5 p-3 text-sm">
            ISO preflight passed: {preflight.distro} · target size {preflight.target_size_gb} GiB.
          </p>
        )}
        {operationError && <p role="alert" className="text-sm text-destructive">{operationError}</p>}

        <Button variant="outline" onClick={checkIso} disabled={!canCheck} loading={checking}>
          {checking ? "Checking ISO…" : "Check ISO"}
        </Button>

        {preflight && (
          <label className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm">
            <input
              type="checkbox"
              className="mt-1 accent-destructive"
              checked={confirmed}
              disabled={building}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            <span>
              Create a new {sizeGb} GiB ZFS image named <strong>{name.trim() || "(enter a name)"}</strong> with login <strong>{username.trim() || "(enter a username)"}</strong>{sudoUser ? " and sudo access" : ""}. The builder will partition and format that new image. If the build fails, the partial image will remain listed so it can be deleted.
            </span>
          </label>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={close} disabled={checking || building}>Cancel</Button>
          <Button variant="primary" onClick={build} disabled={!canBuild} loading={building}>
            {building ? "Building Linux image…" : "Build Image"}
          </Button>
        </DialogFooter>
      </div>
    </Modal>
  );
}
