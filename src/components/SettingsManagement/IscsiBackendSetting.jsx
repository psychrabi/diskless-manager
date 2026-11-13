import { getSettings, saveSettings } from "@/api/modules/system";
import { Card } from "@/components/ui";
import { Label } from "@/components/ui/label";
import { NativeSelect } from "@/components/ui/native-select";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useState } from "react";

const BACKEND_LABELS = {
  targetcli: "targetcli (default)",
  internal_configfs: "Internal iSCSI (configfs)",
};

const IscsiBackendSetting = () => {
  const config = useAppStore((state) => state.appConfig?.settings?.iscsi);
  const fetchConfig = useAppStore((state) => state.fetchConfig);
  const { success, error } = useToastStore();
  const confirm = useConfirm();
  const [busy, setBusy] = useState(false);

  const changeBackend = async (backend) => {
    const current = config?.backend ?? "targetcli";
    if (backend === current) return;

    // The backend decides how every future iSCSI target is created, so the
    // switch is confirmed before it is written.
    const approved = await confirm({
      title: "Change iSCSI backend",
      description: `Switch from ${BACKEND_LABELS[current] ?? current} to ${
        BACKEND_LABELS[backend] ?? backend
      }? Every target provisioned after the restart uses the new backend. Existing targets are not migrated.`,
      confirmText: "Change backend",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });
    if (!approved) return;

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
    <Card title="iSCSI Provisioning Backend" className="h-full">
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
        Backend changes take effect after restarting Diskless Manager. Targets
        already provisioned are not migrated.
      </p>
    </Card>
  );
};

export default IscsiBackendSetting;
