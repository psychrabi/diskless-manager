import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Input } from "../../ui";

const DHCPForm = ({ register, control, errors }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
      <Label htmlFor="dhcp-enabled" className="flex items-center gap-2 text-sm font-medium md:col-span-3">
        <Controller
          name="enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="dhcp-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <span className="ml-2">DHCP Server (Start at boot)</span>
      </Label>
      <Input
        label="Start IP"
        register={register("start_ip")}
        error={errors.start_ip?.message}
        placeholder="192.168.1.100"
      />
      <Input
        label="End IP"
        register={register("end_ip")}
        error={errors.end_ip?.message}
        placeholder="192.168.1.200"
      />
      <Input
        label="Gateway IP"
        register={register("gateway_ip")}
        error={errors.gateway_ip?.message}
        placeholder="192.168.1.1"
      />
      <Input
        label="Subnet IP"
        register={register("subnet_ip")}
        error={errors.subnet_ip?.message}
        placeholder="192.168.1.0"
      />
      <Input
        label="Subnet Mask"
        register={register("subnet_mask")}
        error={errors.subnet_mask?.message}
        placeholder="255.255.255.0"
      />
      <Input
        label="Broadcast IP"
        register={register("broadcast_ip")}
        error={errors.broadcast_ip?.message}
        placeholder="192.168.1.255"
      />
      <Input
        label="DNS Server 1"
        register={register("dns_server1")}
        error={errors.dns_server1?.message}
        placeholder="1.1.1.1"
      />
      <Input
        label="DNS Server 2"
        register={register("dns_server2")}
        error={errors.dns_server2?.message}
        placeholder="8.8.8.8"
      />
      <Input
        label="Next Server IP"
        register={register("next_server_ip")}
        error={errors.next_server_ip?.message}
        placeholder="192.168.1.1"
      />
      <Input
        label="Boot Server IP"
        register={register("boot_server_ip")}
        error={errors.boot_server_ip?.message}
        placeholder="192.168.1.1"
      />
      <Input
        label="Boot Script"
        register={register("boot_script")}
        error={errors.boot_script?.message}
        placeholder="autoexec.ipxe"
      />
      <Input
        label="Legacy Boot File"
        register={register("boot_file_legacy")}
        error={errors.boot_file_legacy?.message}
        placeholder="ipxe.kpxe"
      />
      <Input
        label="UEFI 64 Boot File"
        register={register("boot_file_uefi64")}
        error={errors.boot_file_uefi64?.message}
        placeholder="ipxe.efi"
      />
      <Input
        label="UEFI 32 Boot File"
        register={register("boot_file_uefi32")}
        error={errors.boot_file_uefi32?.message}
        placeholder="ipxe.efi"
      />
    </div>
  );
};

export default DHCPForm;
