import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button } from "../ui";
import { dhcpSchema } from "@/schema";
import DHCPForm from "../SettingsManagement/Forms/DHCPForm";

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
        <DHCPForm register={register} errors={errors} config={initialConfig} />

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
