import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import z from "zod";
import { Button, Input } from "../ui";
import { dhcpSchema } from "@/schema";

const DHCPStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(dhcpSchema),
    defaultValues: initialConfig || {},
  });

  return (
    <div className="space-y-6">
      <div className="text-center space-y-2">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary mb-2">
          <Network className="w-6 h-6" />
        </div>
        <h2 className="text-2xl font-bold">DHCP Server</h2>
        <p className="text-base-content/60 max-w-sm mx-auto">
          Configure the DHCP server to assign IP addresses and boot files to
          your diskless clients.
        </p>
      </div>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div className="grid grid-cols-3 gap-4">
          <label htmlFor="enabled" className="label col-span-3">
            <input
              id="enabled"
              className="checkbox"
              {...register("enabled")}
              type="checkbox"
              defaultChecked={true}
            />
            DHCP Server (Start at boot)
          </label>
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
            label="Legacy Boot File"
            register={register("boot_file_legacy")}
            error={errors.boot_file_legacy?.message}
          />
          <Input
            label="UEFI 64 Boot File"
            register={register("boot_file_uefi64")}
            error={errors.boot_file_uefi64?.message}
          />
          <Input
            label="UEFI 32 Boot File"
            register={register("boot_file_uefi32")}
            error={errors.boot_file_uefi32?.message}
          />
        </div>

        <Button
          type="submit"
          variant="primary"
          className="w-full mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring DHCP..." : "Save & Continue"}
        </Button>
      </form>
    </div>
  );
};

export default DHCPStep;
