import { zodResolver } from "@hookform/resolvers/zod";
import { Globe } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button } from "../ui";
import { httpSchema } from "@/schema";
import HTTPForm from "../SettingsManagement/Forms/HTTPForm";

const HTTPStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    defaultValues: initialConfig || {},
    resolver: zodResolver(httpSchema),
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
        <HTTPForm register={register} errors={errors} config={initialConfig} />

        <Button
          type="submit"
          variant="primary"
          className="w-full mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring HTTP..." : "Save & Continue"}
        </Button>
      </form>
    </div>
  );
};

export default HTTPStep;
