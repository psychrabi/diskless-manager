import React, { useEffect, useState, useCallback } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { serverSchema } from "@/schema";
import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { Card, Button, Input } from "../ui";
import { Monitor, RefreshCcw, Shield, Globe, Network } from "lucide-react";

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
    defaultValues: {
      interface: [],
      ip_address: "",
      netmask: "255.255.255.0",
      gateway: "",
      dns: "8.8.8.8, 8.8.4.4",
      hostname: "",
      domain: "",
    },
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
    if (appConfig?.settings?.server) {
      // Transform the array of DNS IPs into a comma-separated string for the form
      const serverSettings = { ...appConfig.settings.server };
      if (Array.isArray(serverSettings.dns)) {
        serverSettings.dns = serverSettings.dns.join(", ");
      }
      reset(serverSettings);
    }
  }, [appConfig, reset]);

  const onAutoPopulate = useCallback(async () => {
    const data = await detectNetwork();
    if (data) {
      if (data.hostname)
        setValue("hostname", data.hostname, { shouldValidate: true });
      if (data.domain)
        setValue("domain", data.domain, { shouldValidate: true });
      if (data.primary_ip)
        setValue("ip_address", data.primary_ip, { shouldValidate: true });
      if (data.primary_mask)
        setValue("netmask", data.primary_mask, { shouldValidate: true });
      if (data.gateway)
        setValue("gateway", data.gateway, { shouldValidate: true });
      if (data.dns && data.dns.length > 0) {
        // Transform array to string
        setValue("dns", data.dns.join(", "), { shouldValidate: true });
      }
      if (data.primary_interface) {
        setValue("interface", [data.primary_interface], {
          shouldValidate: true,
        });
      }
    }
  }, [detectNetwork, setValue]);

  const onSubmit = async (data) => {
    // Transform comma-separated string back to array of IPs for the backend
    const payload = {
      ...data,
      dns: data.dns
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
    };

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
    <Card title="Server Network Configuration" icon={Network}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* Left Column: Interface & Identification */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <label className="text-sm font-semibold text-base-content/70 uppercase tracking-tight flex items-center gap-2">
                <Network size={14} /> Network Interfaces
              </label>
              <button
                type="button"
                onClick={loadData}
                className="btn btn-ghost btn-xs gap-1 opacity-70 hover:opacity-100"
                disabled={loadingInterfaces}
              >
                <RefreshCcw
                  size={12}
                  className={loadingInterfaces ? "animate-spin" : ""}
                />
                Refresh
              </button>
            </div>
            <div className="border border-base-300 rounded-xl bg-base-200/30 overflow-hidden">
              <div className="max-h-[200px] overflow-y-auto p-2 space-y-1">
                {loadingInterfaces ? (
                  <div className="flex flex-col items-center justify-center py-8 gap-2 opacity-50">
                    <span className="loading loading-spinner loading-sm"></span>
                    <span className="text-xs">Detecting interfaces...</span>
                  </div>
                ) : interfaces.length === 0 ? (
                  <div className="py-8 text-center text-sm text-error/70 italic">
                    No active network interfaces detected.
                  </div>
                ) : (
                  interfaces.map((iface) => {
                    const isSelected = selectedInterfaces?.includes(iface);
                    return (
                      <label
                        key={iface}
                        className={`flex items-center justify-between p-2 rounded-lg cursor-pointer transition-all border ${
                          isSelected
                            ? "bg-primary/10 border-primary/30 text-primary shadow-sm"
                            : "bg-base-100 border-transparent hover:border-base-300 hover:bg-base-200"
                        }`}
                      >
                        <div className="flex items-center gap-3">
                          <input
                            type="checkbox"
                            className="checkbox checkbox-primary checkbox-sm rounded"
                            checked={isSelected}
                            onChange={() => handleInterfaceToggle(iface)}
                          />
                          <span className="font-mono text-sm font-bold">
                            {iface}
                          </span>
                        </div>
                        {isSelected && (
                          <span className="badge badge-primary badge-xs py-2 px-2 font-bold uppercase tracking-widest text-[10px]">
                            Active
                          </span>
                        )}
                      </label>
                    );
                  })
                )}
              </div>
            </div>
            {errors.interface && (
              <p className="text-xs text-error font-medium flex items-center gap-1 mt-1">
                <span>⚠️</span> {errors.interface.message}
              </p>
            )}
          </div>

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
              onClick={() => reset(appConfig?.settings?.server)}
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
