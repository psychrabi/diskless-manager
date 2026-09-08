import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Input } from "../../ui";

const SambaForm = ({ register, control, errors }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Label htmlFor="samba-enabled" className="flex items-center gap-2 text-sm font-medium md:col-span-2">
        <Controller
          name="enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="samba-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <span className="ml-2">Samba Server (Start at boot)</span>
      </Label>
      <div className="flex items-center flex-wrap gap-4 pt-2">
        <Label className="flex items-center space-x-2 cursor-pointer">
          <Controller
            name="guest_ok"
            control={control}
            defaultValue={false}
            render={({ field }) => (
              <Checkbox
                id="samba-guest_ok"
                name={field.name}
                ref={field.ref}
                checked={Boolean(field.value)}
                onCheckedChange={field.onChange}
                onBlur={field.onBlur}
              />
            )}
          />
          <span className="text-sm">Allow guest access</span>
        </Label>
        <Label className="flex items-center space-x-2 cursor-pointer">
          <Controller
            name="read_only"
            control={control}
            defaultValue={false}
            render={({ field }) => (
              <Checkbox
                id="samba-read_only"
                name={field.name}
                ref={field.ref}
                checked={Boolean(field.value)}
                onCheckedChange={field.onChange}
                onBlur={field.onBlur}
              />
            )}
          />
          <span className="text-sm">Read only</span>
        </Label>
      </div>

      <Input
        label="Share Name"
        register={register("share_name")}
        error={errors.share_name?.message}
        placeholder="shared"
        autoComplete="off"
      />
      <Input
        label="Share Path"
        register={register("share_path")}
        error={errors.share_path?.message}
        placeholder="/srv/shared"
        autoComplete="off"
      />
      <Input
        label="Workgroup"
        register={register("workgroup")}
        error={errors.workgroup?.message}
        placeholder="WORKGROUP"
        autoComplete="off"
      />
    </div>
  );
};

export default SambaForm;
