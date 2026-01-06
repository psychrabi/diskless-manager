import { useAppStore } from "@/store/useAppStore";
import { Settings } from "lucide-react";
import { useEffect } from "react";
import { Card } from "../ui";
import BootProcessOverview from "./BootProcessOverview";
import DHCPConfigForm from "./DHCPConfigForm";
import HTTPConfigForm from "./HTTPConfigForm";
import TFTPConfigForm from "./TFTPConfigForm";
import SambaConfigForm from "./SambaConfigForm";
import ISCSIConfig from "./ISCSIConfig";
import NetworkConfig from "./NetworkConfig";

const SettingsManagement = () => {
  const fetchConfig = useAppStore((state) => state.fetchConfig);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  return (
    <Card title="System Settings" icon={Settings} className="bg-base-300">
      <div className="min-h-[calc(100vh-14rem)] space-y-6">
        <div className="grid gap-6 md:grid-cols-2">
          {/* Server Network Configuration */}
          <NetworkConfig />

          {/* DHCP Server Configuration */}
          <DHCPConfigForm />
          {/* TFTP Server Configuration */}
          <TFTPConfigForm />

          {/* TFTP Server Configuration */}
          <HTTPConfigForm />

          {/* Samba Server Configuration */}
          <SambaConfigForm />

          {/* ISCSI Target Configuration */}
          <ISCSIConfig />
        </div>
        {/* Boot Process Overview Card */}
        <BootProcessOverview />
      </div>
    </Card>
  );
};

export default SettingsManagement;
