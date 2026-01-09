import { Card } from "@/components/ui";
import { dhcpSchema } from "@/schema";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import DHCPForm from "../SettingsManagement/Forms/DHCPForm";
import { Button } from "@/components/ui";

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
    <Card title="DHCP Server" subtitle="Configure the DHCP server to assign IP addresses and boot files to
          your diskless clients." icon={Network} className="border-t-4 border-primary overflow-hidden"
    >
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
    </Card>
  );
};

export default DHCPStep;
