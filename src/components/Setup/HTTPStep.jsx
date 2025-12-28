import { Globe } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Input } from "../ui";

const httpInitial = {
  root_dir: "/srv/tftp",
  server_ip: "*",
  port: "80",
};

const HTTPStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    defaultValues: initialConfig || httpInitial,
  });

  return (
    <div className="space-y-6">
      <div className="text-center space-y-2">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary mb-2">
          <Globe className="w-6 h-6" />
        </div>
        <h2 className="text-2xl font-bold">HTTP Server</h2>
        <p className="text-base-content/60 max-w-sm mx-auto">
          Configure the HTTP server to serve larger boot images and assets
          faster than TFTP.
        </p>
      </div>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          label="HTTP Root Directory"
          register={register("root_dir")}
          error={errors.root_dir}
          placeholder="/srv/http"
        />
        <div className="grid grid-cols-2 gap-4">
          <Input
            label="Server IP"
            register={register("server_ip")}
            error={errors.server_ip}
            placeholder="*"
          />
          <Input
            label="Server Port"
            register={register("port")}
            error={errors.port}
            placeholder="80"
          />
        </div>

        <Button
          type="submit"
          variant="primary"
          className="w-full"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring HTTP..." : "Save & Continue"}
        </Button>
      </form>
    </div>
  );
};

export default HTTPStep;
