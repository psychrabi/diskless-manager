import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Input } from "../../ui";

const HTTPForm = ({ register, control, errors }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Label htmlFor="http-enabled" className="flex items-center gap-2 text-sm font-medium md:col-span-2">
        <Controller
          name="enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="http-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <span className="ml-2">HTTP Server (Start at boot)</span>
      </Label>

      <div className="grid grid-cols-2 gap-4 col-span-2">
        <Input
          label="Server IP"
          register={register("server_ip")}
          error={errors.server_ip?.message}
          placeholder="*"
          autoComplete="off"
        />
        <Input
          label="Server Port"
          register={register("port")}
          error={errors.port?.message}
          placeholder="80"
          autoComplete="off"
          inputMode="numeric"
        />
      </div>
      <Input
        className="col-span-2"
        label="HTTP Root Directory"
        register={register("root_dir")}
        error={errors.root_dir?.message}
        placeholder="/srv/http"
        autoComplete="off"
      />
    </div>
  );
};

export default HTTPForm;
