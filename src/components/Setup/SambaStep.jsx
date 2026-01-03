import { zodResolver } from "@hookform/resolvers/zod";
import { Share2 } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button } from "../ui";
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
    <div className="space-y-6">
      <div className="text-center space-y-2">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary mb-2">
          <Share2 className="w-6 h-6" />
        </div>
        <h2 className="text-2xl font-bold">Samba Share</h2>
        <p className="text-base-content/60 max-w-sm mx-auto">
          Configure a default Samba share for your network clients to access
          shared files and games.
        </p>
      </div>

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
    </div>
  );
};

export default SambaStep;
