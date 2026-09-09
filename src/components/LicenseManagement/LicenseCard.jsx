import { useAppStore } from "@/store/useAppStore";
import { Key } from "lucide-react";
import { Card, StatusBadge } from "@/components/ui";

export default function LicenseCard() {
  const license = useAppStore((state) => state.licenseInfo) || {};
  const isActive = Boolean(license.license_status);

  return (
    <Card title="License Information" icon={Key} className="h-full">
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-4">
          <span className="text-sm text-muted-foreground">Status</span>
          <StatusBadge status={isActive ? "success" : "error"} size="sm">
            {isActive ? license.license_status : "Not activated"}
          </StatusBadge>
        </div>
        <div className="flex items-center justify-between gap-4">
          <span className="text-sm text-muted-foreground">Expires</span>
          <span className="text-sm font-medium text-right">{license.license_expires || "\u2014"}</span>
        </div>
        <div className="flex items-center justify-between gap-4">
          <span className="text-sm text-muted-foreground">Key</span>
          <span className=" text-xs text-muted-foreground text-right break-all">
            {license.license_key || "\u2014"}
          </span>
        </div>
      </div>
    </Card>
  );
}
