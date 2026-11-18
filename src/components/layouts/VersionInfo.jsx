import { useAppStore } from "@/store/useAppStore";

// Persistent identity line fed by /api/system/info, mirroring the version,
// build date, and checksum shown by the Windows client GUI.
export default function VersionInfo() {
  const serverInfo = useAppStore((state) => state.serverInfo);
  if (!serverInfo?.app_version) {
    return null;
  }
  const built = serverInfo.build_date ? ` · built ${serverInfo.build_date}` : "";
  return (
    <div
      className="px-2 pb-2 text-[11px] leading-tight text-sidebar-foreground/50 group-data-[collapsible=icon]:hidden"
      data-testid="version-info"
    >
      {`v${serverInfo.app_version}${built}`}
    </div>
  );
}
