import {
  HardDrive,
  History,
  Network,
  RefreshCw,
  Settings,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { useOnClickOutside } from "../../hooks/useOnClickOutside";
import { cn } from "@/lib/utils";
import { isSuperClient } from "@/lib/clientMode";
import { isOnlineStatus } from "@/lib/clientStatus";
import { useAuth } from "@/contexts/auth";
import { Badge } from "@/components/ui/badge";

const MenuItem = ({
  icon: Icon,
  label,
  onClick,
  variant = "default",
  className = "",
  disabled = false,
}) => {
  const variants = {
    default: "hover:bg-accent hover:text-accent-foreground",
    success: "hover:bg-accent hover:text-accent-foreground",
    warning: "hover:bg-accent hover:text-accent-foreground",
    error: "hover:bg-accent hover:text-accent-foreground",
    info: "hover:bg-accent hover:text-accent-foreground",
    secondary: "hover:bg-accent hover:text-accent-foreground",
    destructive:
      "text-destructive hover:bg-destructive hover:text-destructive-foreground",
  };

  const handleKeyDown = (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onClick?.(e);
    }
  };

  return (
    <li role="none">
      <button
        type="button"
        disabled={disabled}
        onClick={onClick}
        onKeyDown={handleKeyDown}
        className={cn(
          "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors duration-150",
          variants[variant],
          className
        )}
      >
        <Icon className="size-4 shrink-0" />
        <span>{label}</span>
      </button>
    </li>
  );
};

const SectionHeader = ({ label }) => (
  <div className="px-3 py-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-widest">
    {label}
  </div>
);

export const ContextMenu = ({
  isOpen,
  xPos,
  yPos,
  targetClient,
  onClose,
  actions,
  onExitLegacySuper,
  superStatus,
  superMedia = [],
  superLoading = false,
  superError = false,
  targetImage,
}) => {
  const menuRef = useRef(null);
  const [coords, setCoords] = useState({ x: xPos, y: yPos });
  const { user } = useAuth();
  useOnClickOutside(menuRef, onClose);

  useLayoutEffect(() => {
    if (isOpen && menuRef.current) {
      const { width, height } = menuRef.current.getBoundingClientRect();
      setCoords({
        x: Math.min(xPos, window.innerWidth - width - 10),
        y: Math.min(yPos, window.innerHeight - height - 10),
      });
    }
  }, [isOpen, xPos, yPos]);

  if (!isOpen || !targetClient) return null;

  const isOnline = isOnlineStatus(targetClient?.status);
  const legacyState = targetClient?.legacy_super_state;
  const isLegacySuper = legacyState === "legacy" || legacyState === "recovery_required" || superStatus?.legacy;
  const session = superStatus?.session;
  const sessionState = session?.state?.toLowerCase().replaceAll("_", "");
  const isManagedSuper = session && session.state !== "completed";
  const isSuper = !isLegacySuper && (isManagedSuper || (superStatus == null && isSuperClient(targetClient)));
  const isUnprovisioned = targetClient?.master === "pending";
  const isPersistent =
    targetClient?.snapshot && targetClient?.keep_writeback && !isSuper;
  const isNonPersistent =
    targetClient?.snapshot && !targetClient?.keep_writeback && !isSuper;
  const mediaType = targetImage?.os_type === "linux" ? "linux_live" : targetImage?.os_type === "windows" ? "winpe" : null;
  const matchingMedia = superMedia.filter((media) => media.media_type === mediaType && media.validation_state === "valid");
  const defaultMedia = matchingMedia.find((media) => media.is_default);
  const mediaTypeLabel = mediaType === "winpe" ? "Windows" : mediaType === "linux_live" ? "Linux" : "this";
  const firmwareVerified = targetClient?.pxe_mode === "bios" ? defaultMedia?.bios_verified : defaultMedia?.uefi_verified;
  const hasReadyAssignment = !!targetImage && targetImage.status === "ready" && !!targetClient?.master && targetClient.master !== "pending";
  const hasPendingRecovery = targetClient?.recovery_required || superStatus?.storage_recovery;
  const canStartNormal = hasReadyAssignment && !isOnline && !targetClient?.mutation_state && !hasPendingRecovery && !superLoading && !!defaultMedia && firmwareVerified;
  const canStartVerification = hasReadyAssignment && !isOnline && !targetClient?.mutation_state && !hasPendingRecovery && !superLoading && !!defaultMedia;
  const sessionCanFinish = !isOnline && !targetClient?.mutation_state && !hasPendingRecovery && sessionState === "active";

  const handleAction = (cb) => {
    cb(targetClient);
    onClose();
  };

  return (
    <div
      ref={menuRef}
      style={{ top: coords.y, left: coords.x }}
      className="fixed z-50 min-w-55 overflow-hidden rounded-xl bg-popover p-1.5 text-popover-foreground shadow-2xl ring-1 ring-foreground/10 backdrop-blur-md"
    >
      <div className="border-b border-border px-3 py-2.5">
        <p className="text-xs font-bold uppercase tracking-widest text-primary mb-0.5">
          {targetClient.name}
        </p>

        <p className="text-[10px] text-muted-foreground  truncate">
          {targetClient.ip} • {targetClient.mac}
        </p>
      </div>

      <ul className="flex w-full flex-col p-0" role="menu">
        {isOnline && user?.role === "admin" && isManagedSuper && (
          <>
            <SectionHeader label="Super session" />
            <div className="px-3 py-1.5 text-xs text-muted-foreground">
              <Badge variant="secondary">Super session: {sessionState === "recoveryrequired" ? "Recovery required" : session?.state?.replaceAll("_", " ")}</Badge>
              <p className="mt-1">Save and Discard are available after the client is offline.</p>
              {hasPendingRecovery && <p className="mt-1 text-destructive">Client storage recovery is pending.</p>}
            </div>
            <MenuItem icon={History} label={session?.verification_only ? "Save unavailable (verification only)" : "Save Super Changes"} disabled />
            <MenuItem icon={ShieldAlert} label="Discard Super Changes" variant="destructive" disabled />
          </>
        )}
        {!isOnline && user?.role === "admin" && (
          <>
            <SectionHeader label="Maintenance" />
            {isLegacySuper ? (
              user?.role === "admin" && (
                <MenuItem
                  icon={ShieldAlert}
                  label="Exit Legacy Super"
                  variant="warning"
                  onClick={() => handleAction(onExitLegacySuper)}
                />
              )
            ) : isManagedSuper ? (
              <>
                <div className="px-3 py-1.5 text-xs text-muted-foreground">
                  <Badge variant={sessionState === "recoveryrequired" ? "destructive" : "secondary"}>
                    Super session: {sessionState === "recoveryrequired" ? "Recovery required" : session?.state?.replaceAll("_", " ")}
                  </Badge>
                  {session?.verification_only && <p className="mt-1">Verification only · changes are disposable</p>}
                  {targetClient.use_game_disk && <p className="mt-1">Use game disk stays enabled; its LUN is temporarily unmapped.</p>}
                  {hasPendingRecovery && <p className="mt-1 text-destructive">Client storage recovery is pending; retry recovery before finishing this session.</p>}
                </div>
                <MenuItem
                  icon={History}
                  label={session?.verification_only ? "Save unavailable (verification only)" : "Save Super Changes"}
                  variant="secondary"
                  disabled={!sessionCanFinish || session?.verification_only}
                  onClick={() => sessionCanFinish && !session?.verification_only && actions.saveSuper(targetClient, superStatus)}
                />
                <MenuItem
                  icon={ShieldAlert}
                  label="Discard Super Changes"
                  variant="destructive"
                  disabled={!sessionCanFinish}
                  onClick={() => sessionCanFinish && actions.discardSuper(targetClient, superStatus)}
                />
              </>
            ) : isSuper ? (
              <div className="px-3 py-2 text-sm text-amber-700 dark:text-amber-300">
                Super session status is unavailable. Refresh client status before taking action.
              </div>
            ) : (
              <>
                <MenuItem
                  icon={HardDrive}
                  label={superLoading ? "Checking installer media…" : canStartNormal ? "Start Super session" : "Super session unavailable"}
                  variant="secondary"
                  disabled={!canStartNormal}
                  onClick={() => canStartNormal && actions.startSuper(targetClient, false, superStatus)}
                />
                <MenuItem
                  icon={ShieldAlert}
                  label={canStartVerification ? "Start verification-only session" : "Verification session unavailable"}
                  variant="warning"
                  disabled={!canStartVerification}
                  onClick={() => canStartVerification && actions.startSuper(targetClient, true, superStatus)}
                />
                {!superLoading && !superError && hasPendingRecovery && <p className="px-3 pb-2 text-xs text-destructive">Client storage recovery is pending; retry recovery before starting a session.</p>}
                {!superLoading && superError && <p className="px-3 pb-2 text-xs text-destructive">Unable to load Super-session or installer-media status. Reopen the menu to retry.</p>}
                {!superLoading && !superError && !hasReadyAssignment && <p className="px-3 pb-2 text-xs text-muted-foreground">An assigned ready image revision is required.</p>}
                {!superLoading && !superError && hasReadyAssignment && !defaultMedia && (
                  <p className="px-3 pb-2 text-xs text-muted-foreground">
                    {matchingMedia.length > 0
                      ? `A valid ${mediaTypeLabel} installer ISO exists but none is marked default; set one as default in Installer Media.`
                      : "No valid installer ISO matches this image."}
                  </p>
                )}
                {!superLoading && defaultMedia && !firmwareVerified && <p className="px-3 pb-2 text-xs text-muted-foreground">Default ISO is not boot-verified for {targetClient?.pxe_mode || "this firmware mode"}; use verification-only first.</p>}
              </>
            )}
          </>
        )}

        {!isOnline && (
          <>
            <SectionHeader label="Management" />
            <MenuItem
              icon={Settings}
              label="Edit Client"
              onClick={() => handleAction(actions.edit)}
            />

            {isUnprovisioned && (
              <MenuItem
                icon={Network}
                label="Request Inventory Boot"
                variant="info"
                onClick={() => handleAction(actions.inventoryBoot)}
              />
            )}

            {isPersistent && (
              <MenuItem
                icon={History}
                label="Reset Writeback"
                variant="warning"
                onClick={() => handleAction(actions.reset)}
              />
            )}
            {isNonPersistent && (
              <MenuItem
                icon={RefreshCw}
                label="Reset to Clean"
                variant="warning"
                onClick={() => handleAction(actions.resetToClean)}
              />
            )}

            <MenuItem
              icon={Trash2}
              label="Delete Client"
              variant="destructive"
              onClick={() => handleAction(actions.delete)}
            />
          </>
        )}
      </ul>
    </div>
  );
};
