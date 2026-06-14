import { useAppStore } from "@/store/useAppStore";
import { Key } from "lucide-react";
import { Card, StatusBadge } from "@/components/ui";

export default function LicenseCard() {
  const license = useAppStore((state) => state.licenseInfo) || {};
  const isActive = Boolean(license.license_status);

  return (
    <Card title="License Information" icon={Key}>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <span className="font-semibold text-sm">Status:</span>
          <StatusBadge status={isActive ? "success" : "error"} size="sm">
            {isActive ? license.license_status : "Not activated"}
          </StatusBadge>
        </div>
        <div className="flex justify-between text-sm">
          <span className="font-semibold">Expires:</span>
          <span className="text-base-content/70">{license.license_expires || "\u2014"}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="font-semibold">Key:</span>
          <span className="font-mono text-xs text-base-content/70">
            {license.license_key || "\u2014"}
          </span>
        </div>
      </div>
    </Card>
  );
}
