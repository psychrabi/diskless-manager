import { Label } from "@/components/ui/label";
import { Alert } from "@/components/ui/alert";
import { useEffect, useState, useCallback } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { serverSchema } from "@/schema";
import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useConfirm } from "@/contexts/confirmDialog";
import { Card, Button, Input } from "@/components/ui";
import { RefreshCcw, Shield, Globe, Network, TriangleAlert } from "lucide-react";
import NetworkInterfaceSelector from "./NetworkInterfaceSelector";

const DEFAULT_FORM_VALUES = {
  interface: [],
  ip_address: "",
  netmask: "255.255.255.0",
  gateway: "",
  dns: "8.8.8.8, 8.8.4.4",
  hostname: "",
  domain: "",
};

const formatServerSettingsForForm = (settings) => {
  if (!settings) return DEFAULT_FORM_VALUES;

  return {
    ...DEFAULT_FORM_VALUES,
    ...settings,
    interface: Array.isArray(settings.interface)
      ? settings.interface
      : settings.interface
      ? [settings.interface]
      : [],
    dns: Array.isArray(settings.dns)
      ? settings.dns.join(", ")
      : settings.dns || DEFAULT_FORM_VALUES.dns,
  };
};

const toServerPayload = (data) => ({
  ...data,
  dns: (data.dns || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean),
});

export default function NetworkConfig({ applyOnSave = true, onSaved }) {
  const { fetchInterfaces, detectNetwork, applyNetworkSettings, updateServer } =
    useSettings();
  const { appConfig, fetchServerInfo } = useAppStore();
  const confirm = useConfirm();
  const [interfaces, setInterfaces] = useState([]);
  const [loadingInterfaces, setLoadingInterfaces] = useState(true);

  const {
    register,
    handleSubmit,
    setValue,
    watch,
    getValues,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(serverSchema),
    defaultValues: DEFAULT_FORM_VALUES,
  });

  const selectedInterfaces = watch("interface");

  const loadData = useCallback(async () => {
    setLoadingInterfaces(true);
    try {
      const ifaces = await fetchInterfaces();
      setInterfaces(ifaces || []);
      await fetchServerInfo();
    } catch (err) {
      console.error("Failed to load network data:", err);
    } finally {
      setLoadingInterfaces(false);
    }
  }, [fetchInterfaces, fetchServerInfo]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    reset(formatServerSettingsForForm(appConfig?.settings?.server));
  }, [appConfig, reset]);

  const onAutoPopulate = useCallback(async () => {
    const data = await detectNetwork();
    if (!data) return;

    const fieldMap = {
      hostname: "hostname",
      domain: "domain",
      primary_ip: "ip_address",
      primary_mask: "netmask",
      gateway: "gateway",
    };

    Object.entries(fieldMap).forEach(([sourceField, targetField]) => {
      if (data[sourceField]) {
        setValue(targetField, data[sourceField], { shouldValidate: true });
      }
    });

    if (data.dns?.length) {
      setValue("dns", data.dns.join(", "), { shouldValidate: true });
    }

    if (data.primary_interface) {
      setValue("interface", [data.primary_interface], {
        shouldValidate: true,
      });
    }
  }, [detectNetwork, setValue]);

  /**
   * Applying rewrites the host network configuration and takes effect
   * immediately, which can drop the admin's own connection. Every apply path
   * goes through this gate so the target interface and resulting address are
   * named before anything is committed.
   */
  const confirmApply = useCallback(
    async (payload) =>
      confirm({
        title: "Apply network configuration",
        description: `This rewrites the server network configuration and applies it immediately on ${
          payload.interface?.length
            ? payload.interface.join(", ")
            : "the primary interface"
        }, moving the server to ${
          payload.ip_address
            ? `${payload.ip_address} / ${payload.netmask}`
            : "the detected address"
        }. Running clients lose their boot session, and the management UI becomes unreachable if the address is wrong.`,
        confirmText: "Apply now",
        cancelText: "Keep it saved",
        confirmVariant: "destructive",
        size: "2xl",
      }),
    [confirm]
  );

  const onSubmit = async (data) => {
    const payload = toServerPayload(data);

    const success = await updateServer(payload);
    if (success) {
      // The configuration is written on save; applying it is a separate,
      // confirmed step because it can sever connectivity.
      if (applyOnSave) {
        const approved = await confirmApply(payload);
        if (approved) await applyNetworkSettings();
      }
      await onSaved?.();
    }
  };

  const handleApplyStatic = async () => {
    const payload = toServerPayload(getValues());
    const approved = await confirmApply(payload);
    if (!approved) return;
    await applyNetworkSettings();
  };

  const handleInterfaceToggle = (iface) => {
    const current = selectedInterfaces || [];
    let next;
    if (current.includes(iface)) {
      next = current.filter((i) => i !== iface);
    } else {
      next = [...current, iface];
    }
    setValue("interface", next, { shouldValidate: true });
  };

  return (
    <Card
      title="Server Network Configuration"
      icon={Network}
      className="h-full"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div data-testid="network-sections" className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <NetworkInterfaceSelector
            loading={loadingInterfaces}
            interfaces={interfaces}
            selectedInterfaces={selectedInterfaces}
            onRefresh={loadData}
            onToggle={handleInterfaceToggle}
            errorMessage={errors.interface?.message}
          />

          {/* Right Column: Addressing */}
          <div className="space-y-3">
            <Label className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <Globe size={14} /> Identification
            </Label>

            <Input
              label="Hostname"
              id="server-hostname"
              register={register("hostname")}
              error={errors.hostname?.message}
              placeholder="pxeserver"
            />
            <Input
              label="Domain"
              id="server-domain"
              register={register("domain")}
              error={errors.domain?.message}
              placeholder="local"
            />
          </div>

          <div className="xl:col-span-2">
            <div className="flex flex-col items-stretch gap-2 sm:flex-row sm:items-center sm:justify-between">
              <Label className="flex items-center gap-2 text-sm font-semibold text-foreground">
                <Shield size={14} /> Addressing (Static IP)
              </Label>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={onAutoPopulate}
                icon={RefreshCcw}
                className="w-full whitespace-nowrap transition-colors hover:bg-primary/20 hover:text-primary sm:w-auto"
                title="Fill from system defaults"
              >
                Auto-Detect
              </Button>
            </div>

            <div className="mt-3 grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Input
                label="IP Address"
                id="server-ip"
                register={register("ip_address")}
                error={errors.ip_address?.message}
                placeholder="192.168.1.1"
              />
              <Input
                label="Subnet Mask"
                id="server-mask"
                register={register("netmask")}
                error={errors.netmask?.message}
                placeholder="255.255.255.0"
              />
            </div>

            <div className="mt-4 space-y-4">
              <Input
                label="Default Gateway"
                id="server-gateway"
                register={register("gateway")}
                error={errors.gateway?.message}
                placeholder="192.168.1.1"
              />

              <Input
                label="DNS Servers (Comma Separated)"
                id="server-dns"
                register={register("dns")}
                error={errors.dns?.message}
                placeholder="8.8.8.8, 8.8.4.4"
                title="Separate multiple IPs with commas"
              />
            </div>

            <Alert className="mt-3 rounded-xl border-none bg-warning/10 px-4 py-3 text-xs leading-relaxed text-warning shadow-sm">
              <div className="flex items-start gap-3">
                <TriangleAlert size={18} className="shrink-0" />
                <span>
                  <strong>Caution:</strong> Applying static IP settings will
                  rewrite the server network configuration
                  (<code>/etc/netplan/99-diskless-manager.yaml</code> on
                  Debian/Ubuntu, a NetworkManager profile on Fedora/RHEL and
                  Arch Linux) and apply changes immediately. Ensure the
                  settings are correct to avoid losing server connectivity.
                  You will be asked to confirm the target interface and
                  address before anything is applied.
                </span>
              </div>
            </Alert>
          </div>
        </div>

        <div data-testid="network-actions" className="flex flex-nowrap items-center justify-end gap-1 pt-4 sm:gap-2">
          <Button
            className="min-w-0 flex-1 shrink px-1 text-center text-xs leading-tight whitespace-normal sm:px-2 sm:text-sm sm:whitespace-nowrap"
            variant="ghost"
            type="button"
            onClick={() => reset(formatServerSettingsForForm(appConfig?.settings?.server))}
            disabled={isSubmitting}
          >
            Reset
          </Button>
          <Button
            className="min-w-0 flex-1 shrink px-1 text-center text-xs leading-tight whitespace-normal sm:px-2 sm:text-sm sm:whitespace-nowrap"
            variant="destructive"
            type="button"
            onClick={handleApplyStatic}
            disabled={isSubmitting}
          >
            Apply as Static IP
          </Button>

          <Button
            className="min-w-0 flex-1 shrink px-1 text-center text-xs leading-tight whitespace-normal sm:px-2 sm:text-sm sm:whitespace-nowrap"
            variant="primary"
            type="submit"
            loading={isSubmitting}
          >
            {isSubmitting
              ? "Saving..."
              : applyOnSave
                ? "Save & Apply"
                : "Save Configuration"}
          </Button>
        </div>
      </form>
    </Card>
  );
}
