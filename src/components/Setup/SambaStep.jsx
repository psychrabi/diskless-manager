import { zodResolver } from "@hookform/resolvers/zod";
import { Share2 } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";
import { sambaSchema } from "@/schema";
import SambaForm from "../SettingsManagement/Forms/SambaForm";

const sambaInitial = {
  share_name: "shared",
  share_path: "/srv/shared",
  read_only: false,
  guest_ok: true,
  workgroup: "WORKGROUP",
  enabled: true,
};

const SambaStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(sambaSchema),
    defaultValues: initialConfig || sambaInitial,
  });

  const handleFormSubmit = (data) => {
    // Backend expects Vec<SambaShare>
    onSubmit([data]);
  };

  return (
    <Card title="Samba Server" subtitle="Configure a default Samba share for your network clients to access
            shared files and games." icon={Share2} className="border-t-4 border-primary overflow-hidden"
    >
      <form onSubmit={handleSubmit(handleFormSubmit)} className="space-y-4">
        <SambaForm
          register={register}
          errors={errors}
          config={initialConfig || sambaInitial}
        />
        <Button
          type="submit"
          variant="primary"
          className="w-full mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring Samba..." : "Save & Continue"}
        </Button>
      </form>
    </Card>
  );
};

export default SambaStep;
