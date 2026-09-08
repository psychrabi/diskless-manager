import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Input } from "../../ui";

const ISCSIForm = ({ register, control, errors }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Label htmlFor="iscsi-enabled" className="flex items-center gap-2 text-sm font-medium md:col-span-2">
        <Controller
          name="enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="iscsi-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <span className="ml-2">ISCSI Server (Start at boot)</span>
      </Label>
      <div className="grid grid-cols-2 gap-4 col-span-2">
        <Input
          label="ISCSI Target Prefix"
          register={register("target_prefix")}
          error={errors.target_prefix?.message}
          placeholder="iqn.2024-01.com.example"
          autoComplete="off"
        />
        <Input
          label="Portal Port"
          register={register("portal_port")}
          error={errors.portal_port?.message}
          placeholder="3260"
          autoComplete="off"
          inputMode="numeric"
        />
      </div>
      <Input
        className="col-span-2"
        label="Targets Directory"
        register={register("targets_dir")}
        error={errors.targets_dir?.message}
        placeholder="/var/lib/iscsi-targets"
        autoComplete="off"
      />
    </div>
  );
};

export default ISCSIForm;
