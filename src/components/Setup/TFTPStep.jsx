import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import { tftpSchema } from "@/schema";
import TFTPForm from "../SettingsManagement/Forms/TFTPForm";

const tftpInitial = {
  root_dir: "/srv/tftp",
  server_ip: "0.0.0.0",
  port: 69,
  options: "--secure",
  enabled: true,
};

const TFTPStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(tftpSchema),
    defaultValues: initialConfig || tftpInitial,
  });

  return (
    <Card title="TFTP Server" subtitle="Configure the TFTP server to serve boot files (iPXE, kernels, etc.) to
            your clients." icon={Network} className="border-t-4 border-primary overflow-hidden"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <TFTPForm
          register={register}
          errors={errors}
          config={initialConfig || tftpInitial}
        />
        <Button
          type="submit"
          variant="primary"
          className="w-full mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring TFTP..." : "Save & Continue"}
        </Button>
      </form>
    </Card>
  );
};

export default TFTPStep;
