import { useSettings } from "@/hooks/useSettings";
import { sambaSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";

export default function SambaConfigForm() {
  const { info } = useToastStore();
  const { updateSamba } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.samba || {},
    resolver: zodResolver(sambaSchema),
  });

  const onSubmit = async (data) => {
    console.log(data);
    info(`Updating Samba Configurations`);
    await updateSamba(data);
  };

  return (
    <Card title="Samba Configuration" icon={Network} className="">
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="grid grid-cols-2 gap-4">
          <label htmlFor="enabled" className="label col-span-2">
            <input
              id="enabled"
              className="checkbox"
              {...register("enabled")}
              type="checkbox"
              defaultChecked={config?.settings?.samba?.enabled}
            />
            Samba Server (Start at boot)
          </label>
          <label htmlFor="guest_ok" className="label">
            <input
              id="guest_ok"
              className="checkbox"
              {...register("guest_ok")}
              type="checkbox"
              defaultChecked={config?.settings?.samba?.guest_ok}
            />
            Allow guest access
          </label>
          <label htmlFor="read_only" className="label">
            <input
              id="read_only"
              className="checkbox"
              {...register("read_only")}
              type="checkbox"
              defaultChecked={config?.settings?.samba?.read_only}
            />
            Read only
          </label>

          <Input
            id="share_name"
            register={register("share_name")}
            label="Share Name"
            className="w-full"
            placeholder="shared"
            error={errors.share_name?.message}
          />
          <Input
            id="share_path"
            register={register("share_path")}
            label="Share Path"
            className="w-full"
            placeholder="/srv/shared"
            error={errors.share_path?.message}
          />
          <Input
            id="workgroup"
            register={register("workgroup")}
            label="Workgroup"
            className="w-full"
            placeholder="WORKGROUP"
            error={errors.workgroup?.message}
          />
        </div>
        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save Samba Settings"}
        </Button>
      </form>
    </Card>
  );
}
