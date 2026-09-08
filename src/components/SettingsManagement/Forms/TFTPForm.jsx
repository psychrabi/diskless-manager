import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Input } from "../../ui";

const TFTPForm = ({ register, control, errors }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Label htmlFor="tftp-enabled" className="flex items-center gap-2 text-sm font-medium md:col-span-2">
        <Controller
          name="enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="tftp-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <span className="ml-2">TFTP Server (Start at boot)</span>
      </Label>

      <Input
        label="TFTP Server IP"
        register={register("server_ip")}
        error={errors.server_ip?.message}
        placeholder="0.0.0.0"
        autoComplete="off"
      />
      <Input
        label="TFTP Server Port"
        register={register("port")}
        error={errors.port?.message}
        placeholder="69"
        autoComplete="off"
        inputMode="numeric"
      />
      <Input
        label="TFTP Options"
        register={register("options")}
        error={errors.options?.message}
        placeholder="--secure"
        autoComplete="off"
      />

      <Input
        label="TFTP Root Directory"
        register={register("root_dir")}
        error={errors.root_dir?.message}
        placeholder="/srv/tftp"
        autoComplete="off"
      />
    </div>
  );
};

export default TFTPForm;
