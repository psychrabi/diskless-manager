import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button } from "../ui";
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
    <div className="space-y-6">
      <div className="text-center space-y-2">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary mb-2">
          <Network className="w-6 h-6" />
        </div>
        <h2 className="text-2xl font-bold">TFTP Server</h2>
        <p className="text-base-content/60 max-w-sm mx-auto">
          Configure the TFTP server to serve boot files (iPXE, kernels, etc.) to
          your clients.
        </p>
      </div>

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
    </div>
  );
};

export default TFTPStep;
