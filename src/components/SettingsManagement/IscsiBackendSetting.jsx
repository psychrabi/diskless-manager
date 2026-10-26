import { getSettings, saveSettings } from "@/api/modules/system";
import { Card } from "@/components/ui";
import { Label } from "@/components/ui/label";
import { NativeSelect } from "@/components/ui/native-select";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useState } from "react";

const IscsiBackendSetting = () => {
  const config = useAppStore((state) => state.appConfig?.settings?.iscsi);
  const fetchConfig = useAppStore((state) => state.fetchConfig);
  const { success, error } = useToastStore();
  const [busy, setBusy] = useState(false);

  const changeBackend = async (backend) => {
    setBusy(true);
    try {
      const settings = await getSettings();
      await saveSettings({ ...settings, iscsi: { ...settings.iscsi, backend } });
      await fetchConfig();
      success("iSCSI backend", "Saved. Restart Diskless Manager to apply the backend change.");
    } catch (e) {
      error("iSCSI backend", e?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title="iSCSI Provisioning Backend" className="h-full xl:col-span-2">
      <Label htmlFor="iscsi-backend">Backend</Label>
      <NativeSelect
        id="iscsi-backend"
        value={config?.backend ?? "targetcli"}
        disabled={busy}
        onChange={(event) => changeBackend(event.target.value)}
        className="mt-2"
      >
        <option value="targetcli">targetcli (default)</option>
        <option value="internal_configfs">Internal iSCSI (configfs)</option>
      </NativeSelect>
      <p className="mt-2 text-sm text-muted-foreground">
        Backend changes take effect after restarting Diskless Manager.
      </p>
    </Card>
  );
};

export default IscsiBackendSetting;
