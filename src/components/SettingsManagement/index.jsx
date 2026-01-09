import { useAppStore } from "@/store/useAppStore";
import { Settings } from "lucide-react";
import { useEffect } from "react";
import { Card } from "@/components/ui";
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
    <Card title="System Settings" subtitle="Manage diskless boot server configurations" icon={Settings} className="bg-base-300">

      <div className="grid gap-4 xl:grid-cols-4 mb-4">
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
    </Card>
  );
};

export default SettingsManagement;
