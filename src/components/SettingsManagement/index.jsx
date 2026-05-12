import { useAppStore } from "@/store/useAppStore";
import { Settings } from "lucide-react";
import { useEffect } from "react";
import { Card } from "@/components/ui";
import ConfigForm from "./ConfigForm";
import { dhcpSchema, tftpSchema, httpSchema, sambaSchema, iscsiSchema } from "@/schema";
import DHCPForm from "./Forms/DHCPForm";
import TFTPForm from "./Forms/TFTPForm";
import HTTPForm from "./Forms/HTTPForm";
import SambaForm from "./Forms/SambaForm";
import ISCSIForm from "./Forms/ISCSIForm";
import BootProcessOverview from "./BootProcessOverview";
import NetworkConfig from "./NetworkConfig";

const SECTIONS = [
  { section: "dhcp", title: "DHCP Server", schema: dhcpSchema, Form: DHCPForm },
  { section: "tftp", title: "TFTP", schema: tftpSchema, Form: TFTPForm },
  { section: "http", title: "HTTP", schema: httpSchema, Form: HTTPForm },
  { section: "samba", title: "Samba", schema: sambaSchema, Form: SambaForm },
  { section: "iscsi", title: "ISCSI", schema: iscsiSchema, Form: ISCSIForm },
];

const SettingsManagement = () => {
  const fetchConfig = useAppStore((state) => state.fetchConfig);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  return (
    <Card title="System Settings" subtitle="Manage diskless boot server configurations" icon={Settings} className="bg-base-300">

      <div className="grid gap-4 xl:grid-cols-4 mb-4">
        <NetworkConfig />
        {SECTIONS.map(({ section, title, schema, Form }) => (
          <ConfigForm
            key={section}
            section={section}
            title={title}
            schema={schema}
            FormComponent={Form}
          />
        ))}
      </div>
      <BootProcessOverview />
    </Card>
  );
};

export default SettingsManagement;
