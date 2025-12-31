import { useSettings } from "@/hooks/useSettings";
import { httpSchema } from "@/schema";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { Network } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card, Input } from "../ui";

export default function HTTPConfigForm() {
  const { info } = useToastStore();
  const { updateHttp } = useSettings();
  const config = useAppStore((state) => state.appConfig);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    defaultValues: config?.settings?.http || {},
    resolver: zodResolver(httpSchema),
  });

  const onSubmit = async (data) => {
    info(`Updating HTTP Configurations`);
    await updateHttp(data);
  };

  return (
    <Card title="HTTP Configuration" icon={Network} className="">
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="grid grid-cols-2 gap-4">
          <label htmlFor="enabled" className="label col-span-2">
            <input
              id="enabled"
              className="checkbox"
              {...register("enabled")}
              type="checkbox"
              defaultChecked={config?.settings?.http?.enabled}
            />
            HTTP Server (Start at boot)
          </label>
          <Input
            id="root_dir"
            register={register("root_dir")}
            label="HTTP Root Directory"
            placeholder="/srv/http"
            error={errors.root_dir?.message}
          />
          <Input
            id="server_ip"
            register={register("server_ip")}
            label="HTTP Server IP"
            placeholder="*"
            error={errors.server_ip?.message}
          />
          <Input
            id="port"
            register={register("port")}
            label="HTTP Server Port"
            placeholder="80"
            error={errors.port?.message}
          />
        </div>
        <Button
          variant="primary"
          type="submit"
          className="mt-4"
          loading={isSubmitting}
        >
          {isSubmitting ? "Saving..." : "Save HTTP Settings"}
        </Button>
      </form>
    </Card>
  );
}
