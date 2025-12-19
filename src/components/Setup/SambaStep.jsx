import { Share2 } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Input } from "../ui";

const sambaInitial = {
  name: "game",
  path: "/storage/diskless/game",
  read_only: false,
  guest_ok: true,
};

const SambaStep = ({ onSubmit, isSubmitting, initialConfig }) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
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
        <Input
          label="Share Name"
          register={register("name")}
          error={errors.name}
          placeholder="game"
        />
        <Input
          label="Share Path"
          register={register("path")}
          error={errors.path}
          placeholder="/srv/samba/share"
        />
        <div className="flex items-center space-x-6 pt-2">
          <label className="flex items-center space-x-2 cursor-pointer">
            <input
              type="checkbox"
              {...register("read_only")}
              className="checkbox checkbox-primary checkbox-sm"
            />
            <span className="text-sm">Read Only</span>
          </label>
          <label className="flex items-center space-x-2 cursor-pointer">
            <input
              type="checkbox"
              {...register("guest_ok")}
              className="checkbox checkbox-primary checkbox-sm"
            />
            <span className="text-sm">Guest OK</span>
          </label>
        </div>

        <Button
          type="submit"
          variant="primary"
          className="w-full"
          loading={isSubmitting}
        >
          {isSubmitting ? "Configuring Samba..." : "Save & Continue"}
        </Button>
      </form>
    </div>
  );
};

export default SambaStep;
