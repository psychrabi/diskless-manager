import { useAppStore } from "@/store/useAppStore";
import { useEffect } from "react";
import { PageHeader } from "@/components/ui";
import ConfigForm from "./ConfigForm";
import { dhcpSchema, tftpSchema, httpSchema, sambaSchema, iscsiSchema } from "@/schema";
import DHCPForm from "./Forms/DHCPForm";
import TFTPForm from "./Forms/TFTPForm";
import HTTPForm from "./Forms/HTTPForm";
import SambaForm from "./Forms/SambaForm";
import ISCSIForm from "./Forms/ISCSIForm";
import BootProcessOverview from "./BootProcessOverview";
import EnrollmentControl from "./EnrollmentControl";
import NetworkConfig from "./NetworkConfig";
import { clientLifecycleSchema } from "@/schema";
import ClientLifecycleForm from "./Forms/ClientLifecycleForm";

const SECTIONS = [
  { section: "client_lifecycle", title: "Client Reset", schema: clientLifecycleSchema, Form: ClientLifecycleForm },
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
    <div className="flex flex-col gap-4">
      <PageHeader
        title="System Settings"
        description="Manage diskless boot server configurations."
      />
      <div className="grid grid-cols-1 items-stretch gap-4 md:grid-cols-2 xl:grid-cols-6">
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
      <EnrollmentControl />
      <BootProcessOverview />
    </div>
  );
};

export default SettingsManagement;
