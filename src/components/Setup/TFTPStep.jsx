import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Input } from "../ui";

const tftpInitial = {
  tftp_root: "/srv/tftp",
  tftp_server_ip: "0.0.0.0",
  tftp_options: "--secure",
};

const TFTPStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
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
        <Input
          label="TFTP Root Directory"
          register={register("tftp_root")}
          error={errors.tftp_root}
          placeholder="/srv/tftp"
        />
        <Input
          label="TFTP Server IP"
          register={register("tftp_server_ip")}
          error={errors.tftp_server_ip}
          placeholder="0.0.0.0"
        />
        <Input
          label="TFTP Options"
          register={register("tftp_options")}
          error={errors.tftp_options}
          placeholder="--secure"
        />

        <Button
          type="submit"
          variant="primary"
          className="w-full"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring TFTP..." : "Save & Continue"}
        </Button>
      </form>
    </div>
  );
};

export default TFTPStep;
