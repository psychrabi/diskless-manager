import React, { useEffect, useState, useCallback } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { serverSchema } from "@/schema";
import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { Card, Button, Input } from "@/components/ui";
import { RefreshCcw, Shield, Globe, Network } from "lucide-react";
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

export default function NetworkConfig() {
  const { fetchInterfaces, detectNetwork, applyNetworkSettings, updateServer } =
    useSettings();
  const { appConfig, fetchServerInfo } = useAppStore();
  const [interfaces, setInterfaces] = useState([]);
  const [loadingInterfaces, setLoadingInterfaces] = useState(true);

  const {
    register,
    handleSubmit,
    setValue,
    watch,
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

  const onSubmit = async (data) => {
    const payload = toServerPayload(data);

    const success = await updateServer(payload);
    if (success) {
      // Apply the network settings and regenerate service configs immediately after saving
      await applyNetworkSettings();
    }
  };

  const handleApplyStatic = async () => {
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
      className="xl:col-span-2"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <NetworkInterfaceSelector
            loading={loadingInterfaces}
            interfaces={interfaces}
            selectedInterfaces={selectedInterfaces}
            onRefresh={loadData}
            onToggle={handleInterfaceToggle}
            errorMessage={errors.interface?.message}
          />

          {/* Right Column: Addressing */}
          <div className="">
            <label className="text-sm font-semibold text-base-content/70 uppercase tracking-tight flex items-center gap-2 ">
              <Globe size={14} /> Identification
            </label>

            <Input
              label="Hostname"
              id="server-hostname"
              register={register("hostname")}
              error={errors.hostname?.message}
              placeholder="pxeserver"
              className="fieldset-compact"
            />
            <Input
              label="Domain"
              id="server-domain"
              register={register("domain")}
              error={errors.domain?.message}
              placeholder="local"
              className="fieldset-compact"
            />
          </div>

          <div className="col-span-2">
            <div className="flex items-center justify-between">
              <label className="text-sm font-semibold text-base-content/70 uppercase tracking-tight flex items-center gap-2">
                <Shield size={14} /> Addressing (Static IP)
              </label>
              <Button
                type="button"
                variant="ghost"
                size="xs"
                onClick={onAutoPopulate}
                className="gap-1.5 h-7 px-2 hover:bg-primary/20 hover:text-primary transition-colors text-xs"
                title="Fill from system defaults"
              >
                <RefreshCcw size={12} />
                Auto-Detect
              </Button>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Input
                label="IP Address"
                id="server-ip"
                register={register("ip_address")}
                error={errors.ip_address?.message}
                placeholder="192.168.1.1"
                className="fieldset-compact"
              />
              <Input
                label="Subnet Mask"
                id="server-mask"
                register={register("netmask")}
                error={errors.netmask?.message}
                placeholder="255.255.255.0"
                className="fieldset-compact"
              />
            </div>

            <Input
              label="Default Gateway"
              id="server-gateway"
              register={register("gateway")}
              error={errors.gateway?.message}
              placeholder="192.168.1.1"
              className="fieldset-compact"
            />

            <div className="space-y-2">
              <Input
                label="DNS Servers (Comma Separated)"
                id="server-dns"
                register={register("dns")}
                error={errors.dns?.message}
                placeholder="8.8.8.8, 8.8.4.4"
                className="fieldset-compact"
                title="Separate multiple IPs with commas"
              />
            </div>

            <div className="alert alert-warning py-3 px-4 mt-3 rounded-xl border-none bg-warning/10 text-warning text-xs leading-relaxed shadow-sm">
              <div className="flex items-start gap-3">
                <span className="text-lg">⚠️</span>
                <span>
                  <strong>Caution:</strong> Applying static IP settings will
                  rewrite
                  <code>/etc/netplan/99-diskless-manager.yaml</code> and apply
                  changes immediately. Ensure the settings are correct to avoid
                  losing server connectivity.
                </span>
              </div>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-between pt-6 gap-4">
          <div className="flex gap-2">
            <Button
              variant="ghost"
              type="button"
              onClick={() => reset(formatServerSettingsForForm(appConfig?.settings?.server))}
              disabled={isSubmitting}
            >
              Reset
            </Button>
            <Button
              variant="outline"
              type="button"
              onClick={handleApplyStatic}
              disabled={isSubmitting}
              className="border-warning/30 hover:bg-warning/10 hover:border-warning/50 text-warning"
            >
              Apply as Static IP
            </Button>
          </div>

          <Button
            variant="primary"
            type="submit"
            loading={isSubmitting}
            className="shadow-lg shadow-primary/20 px-8"
          >
            {isSubmitting ? "Saving..." : "Save Configuration"}
          </Button>
        </div>
      </form>
    </Card>
  );
}
