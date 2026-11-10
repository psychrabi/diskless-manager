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
  { section: "storage", title: "Storage", schema: storagePrewarmSchema, Form: StoragePrewarmForm },
  { section: "windows_servicing", title: "Windows Servicing", schema: windowsServicingSchema, Form: WindowsServicingForm },
  { section: "dhcp", title: "DHCP Server", schema: dhcpSchema, Form: DHCPForm },
  { section: "tftp", title: "TFTP", schema: tftpSchema, Form: TFTPForm },
  { section: "http", title: "HTTP", schema: httpSchema, Form: HTTPForm },
  { section: "samba", title: "Samba", schema: sambaSchema, Form: SambaForm },
  { section: "iscsi", title: "iSCSI", schema: iscsiSchema, Form: ISCSIForm },
];

const NAV = [
  { id: "network", label: "Network" },
  { id: "iscsi-backend", label: "iSCSI backend" },
  { id: "client_lifecycle", label: "Client reset" },
  { id: "client_features", label: "Client features" },
  { id: "storage", label: "Storage" },
  { id: "windows_servicing", label: "Windows servicing" },
  { id: "dhcp", label: "DHCP" },
  { id: "tftp", label: "TFTP" },
  { id: "http", label: "HTTP" },
  { id: "samba", label: "Samba" },
  { id: "iscsi", label: "iSCSI service" },
  { id: "enrollment", label: "Enrollment" },
  { id: "boot-process", label: "Boot process" },
];

const SettingsManagement = () => {
  const fetchConfig = useAppStore((state) => state.fetchConfig);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="System Settings"
        description="Network, storage, and boot services. Changes apply to the server, not to individual clients."
      />
      <div className="grid gap-6 xl:grid-cols-[210px_minmax(0,1fr)]">
        <aside className="hidden xl:block">
          <nav aria-label="Settings sections" className="sticky top-2 flex flex-col gap-0.5 rounded-2xl border border-border/80 bg-card p-2 shadow-[0_1px_2px_rgb(0_0_0/0.03)]">
            <p className="px-2.5 pt-2 pb-1 text-xs font-medium text-muted-foreground">On this page</p>
            {NAV.map((item) => (
              <a
                key={item.id}
                href={`#${item.id}`}
                className="rounded-lg px-2.5 py-1.5 text-[12px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              >
                {item.label}
              </a>
            ))}
          </nav>
        </aside>

        <div className="min-w-0 space-y-5">
          <section id="network" className="scroll-mt-6">
            <NetworkConfig />
          </section>

          <div className="grid gap-5 2xl:grid-cols-2">
            <section id="iscsi-backend" className="scroll-mt-6">
              <IscsiBackendSetting />
            </section>
            {SECTIONS.map(({ section, title, schema, Form }) => (
              <section key={section} id={section} className="scroll-mt-6">
                <ConfigForm
                  section={section}
                  title={title}
                  schema={schema}
                  FormComponent={Form}
                />
              </section>
            ))}
          </div>

          <div className="grid gap-5 2xl:grid-cols-2">
            <section id="enrollment" className="scroll-mt-6">
              <EnrollmentControl />
            </section>
            <section id="boot-process" className="scroll-mt-6">
              <BootProcessOverview />
            </section>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsManagement;
