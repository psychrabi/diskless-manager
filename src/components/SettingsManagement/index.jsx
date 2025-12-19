import { useAppStore } from "@/store/useAppStore";
import { Settings } from "lucide-react";
import { useEffect } from "react";
import { Card } from "../ui";
import BootProcessOverview from "./BootProcessOverview";
import DHCPConfigForm from "./DHCPConfigForm";
import HTTPConfigForm from "./HTTPConfigForm";
import TFTPConfigForm from "./TFTPConfigForm";

const SettingsManagement = () => {
  const fetchConfig = useAppStore((state) => state.fetchConfig);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  return (
    <Card title="System Settings" icon={Settings} className="bg-base-300">
      <div className="min-h-[calc(100vh-14rem)] space-y-6">
        {/* DHCP Server Configuration */}
        <DHCPConfigForm />
        <div className="grid gap-6 md:grid-cols-2">
          {/* TFTP Server Configuration */}
          <TFTPConfigForm />

          {/* TFTP Server Configuration */}
          <HTTPConfigForm />
        </div>
        {/* Boot Process Overview Card */}
        <BootProcessOverview />
      </div>
    </Card>
  );
};

export default SettingsManagement;
