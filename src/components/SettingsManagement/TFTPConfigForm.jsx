import { useSettings } from "@/hooks/useSettings";
import { tftpSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";

export default function TFTPConfigForm() {
  const { info } = useToastStore();
  const { updateTftp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.tftp || {},
    resolver: zodResolver(tftpSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating TFTP Configurations`);
    await updateTftp(data);
  };

  return (
    <Card title="TFTP Configuration" icon={Network} className="">
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="grid grid-cols-2 gap-4">
          <label htmlFor="enabled" className="label col-span-2">
            <input
              id="enabled"
              className="checkbox"
              {...register("enabled")}
              type="checkbox"
              defaultChecked={config?.settings?.tftp?.enabled}
            />
            TFTP Server (Start at boot)
          </label>

          <Input
            id="root_dir"
            register={register("root_dir")}
            label="TFTP Root Directory"
            className="w-full"
            placeholder="/srv/tftp"
            error={errors.root_dir?.message}
          />
          <Input
            id="server_ip"
            register={register("server_ip")}
            label="TFTP Server IP"
            className="w-full"
            placeholder="0.0.0.0"
            error={errors.server_ip?.message}
          />
          <Input
            id="port"
            register={register("port")}
            label="TFTP Server Port"
            className="w-full"
            placeholder="69"
            error={errors.port?.message}
          />
          <Input
            id="options"
            register={register("options")}
            label="TFTP Options"
            className="w-full"
            placeholder="--secure"
            error={errors.options?.message}
          />
        </div>
        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save TFTP Settings"}
        </Button>
      </form>
    </Card>
  );
}
