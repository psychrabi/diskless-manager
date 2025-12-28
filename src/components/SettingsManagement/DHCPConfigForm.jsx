import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Card, Input } from "../ui";

const dhcpSchema = z.object({
  enabled: z.boolean(),
  subnet_ip: z.ipv4(),
  start_ip: z.ipv4(),
  end_ip: z.ipv4(),
  subnet_mask: z.ipv4(),
  gateway_ip: z.ipv4(),
  dns_server1: z.ipv4(),
  dns_server2: z.ipv4(),
  broadcast_ip: z.ipv4(),
  next_server_ip: z.ipv4(),
  boot_server_ip: z.ipv4(),
  boot_script: z.string().optional(),
  boot_file_legacy: z.string().optional(),
  boot_file_uefi32: z.string().optional(),
  boot_file_uefi64: z.string().optional(),
});

const dhcpInitial = {
  enabled: true,
  subnet_ip: "192.168.1.0",
  start_ip: "192.168.1.120",
  end_ip: "192.168.1.130",
  subnet_mask: "255.255.255.0",
  gateway_ip: "192.168.1.254",
  dns_server1: "1.1.1.1",
  dns_server2: "1.0.0.1",
  broadcast_ip: "192.168.1.255",
  next_server_ip: "192.168.1.250",
  boot_server_ip: "192.168.1.250",
  boot_script: "autoexec.ipxe",
  boot_file_legacy: "ipxe.kpxe",
  boot_file_uefi32: "ipxe.efi",
  boot_file_uefi64: "ipxe.efi",
};

// ...

export default function DHCPConfigForm() {
  const { info } = useToastStore();
  const { updateDhcp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(dhcpSchema),
    defaultValues: dhcpInitial,
  });

  // Load saved config when config from store changes
  useEffect(() => {
    console.log(config);

    if (config?.settings?.dhcp) {
      reset(config.settings.dhcp);
      console.log(config);
    } else {
      reset(dhcpInitial);
    }
  }, [config, reset]);

  const onSubmit = async (data) => {
    info(`Updating DHCP Configurations`);
    await updateDhcp(data);
  };

  return (
    <Card title="DHCP Server Configuration" icon={Network}>
      {Object.keys(errors).length > 0 && (
        <div className="mb-4 text-red-500 text-sm">
          {Object.entries(errors).map(([field, error]) => (
            <div key={field}>{error.message}</div>
          ))}
        </div>
      )}
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
          <input
            label="Enabled"
            id="enabled"
            className="checkbox"
            {...register("enabled")}
            type="checkbox"
            defaultChecked={dhcpInitial.enabled}
          />
          <Input
            label="DHCP Start IP"
            id="dhcpstart_ip"
            register={register("start_ip")}
            placeholder="192.168.1.100"
            error={errors.start_ip?.message}
          />
          <Input
            label="DHCP End IP"
            id="dhcpend_ip"
            register={register("end_ip")}
            placeholder="192.168.1.200"
            error={errors.end_ip?.message}
          />
          <Input
            label="Subnet Mask"
            id="subnet_mask"
            register={register("subnet_mask")}
            placeholder="255.255.255.0"
            error={errors.subnet_mask?.message}
          />
          <Input
            label="Gateway IP"
            id="gateway_ip"
            register={register("gateway_ip")}
            placeholder="192.168.1.1"
            error={errors.gateway_ip?.message}
          />
          <Input
            label="DNS Server 1"
            id="dns_server1"
            register={register("dns_server1")}
            placeholder="1.1.1.1"
            error={errors.dns_server1?.message}
          />
          <Input
            label="DNS Server 2"
            id="dns_server2"
            register={register("dns_server2")}
            placeholder="1.0.0.1"
            error={errors.dns_server2?.message}
          />
          <Input
            label="Subnet IP"
            id="subnet_ip"
            register={register("subnet_ip")}
            placeholder="192.168.1.0"
            error={errors.subnet_ip?.message}
          />
          <Input
            label="Boot Server IP"
            id="boot_server_ip"
            register={register("boot_server_ip")}
            placeholder="192.168.1.1"
            error={errors.boot_server_ip?.message}
          />
          <Input
            label="Broadcast IP"
            id="broadcast_ip"
            register={register("broadcast_ip")}
            placeholder="192.168.1.1"
            error={errors.broadcast_ip?.message}
          />
          <Input
            label="Next Server IP"
            id="next_server_ip"
            register={register("next_server_ip")}
            placeholder="192.168.1.1"
            error={errors.next_server_ip?.message}
          />
          <Input
            label="Boot Script"
            id="boot_script"
            register={register("boot_script")}
            placeholder="autoexec.ipxe"
            error={errors.boot_script?.message}
          />
          <Input
            label="Boot File Legacy"
            id="boot_file_legacy"
            register={register("boot_file_legacy")}
            placeholder="ipxe.kpxe"
            error={errors.boot_file_legacy?.message}
          />
          <Input
            label="Boot File UEFI32"
            id="boot_file_uefi32"
            register={register("boot_file_uefi32")}
            placeholder="ipxe.efi"
            error={errors.boot_file_uefi32?.message}
          />
          <Input
            label="Boot File UEFI64"
            id="boot_file_uefi64"
            register={register("boot_file_uefi64")}
            placeholder="ipxe.efi"
            error={errors.boot_file_uefi64?.message}
          />
        </div>
        <Button variant="primary" type="submit" disabled={isSubmitting}>
          {isSubmitting ? "Saving..." : "Save DHCP Settings"}
        </Button>
      </form>
    </Card>
  );
}
