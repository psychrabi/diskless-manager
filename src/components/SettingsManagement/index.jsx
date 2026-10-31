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
import { clientFeaturesSchema, clientLifecycleSchema, storagePrewarmSchema, windowsServicingSchema } from "@/schema";
import ClientLifecycleForm from "./Forms/ClientLifecycleForm";
import ClientFeaturesForm from "./Forms/ClientFeaturesForm";
import StoragePrewarmForm from "./Forms/StoragePrewarmForm";
import WindowsServicingForm from "./Forms/WindowsServicingForm";
import IscsiBackendSetting from "./IscsiBackendSetting";

const SECTIONS = [
  { section: "client_lifecycle", title: "Client Reset", schema: clientLifecycleSchema, Form: ClientLifecycleForm },
  { section: "client_features", title: "Client Features", schema: clientFeaturesSchema, Form: ClientFeaturesForm },
  { section: "storage", title: "Storage Prewarming", schema: storagePrewarmSchema, Form: StoragePrewarmForm },
  { section: "windows_servicing", title: "Windows Servicing", schema: windowsServicingSchema, Form: WindowsServicingForm },
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
        <IscsiBackendSetting />

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
