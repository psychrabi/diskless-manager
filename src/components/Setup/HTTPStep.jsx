import { zodResolver } from "@hookform/resolvers/zod";
import { Globe } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
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
    <Card title="HTTP Server" subtitle="Configure the HTTP server to serve larger boot images and assets
          faster than TFTP." icon={Globe} className="border-t-4 border-primary overflow-hidden"
    >
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
    </Card>
  );
};

export default HTTPStep;
