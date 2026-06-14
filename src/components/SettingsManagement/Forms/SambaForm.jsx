import { Input } from "../../ui";

const SambaForm = ({ register, errors, config }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <label htmlFor="enabled" className="label md:col-span-2">
        <input
          id="enabled"
          className="checkbox checkbox-primary"
          {...register("enabled")}
          type="checkbox"
          defaultChecked={config?.enabled}
        />
        <span className="ml-2">Samba Server (Start at boot)</span>
      </label>
      <div className="flex items-center space-x-6 pt-2">
        <label className="flex items-center space-x-2 cursor-pointer">
          <input
            id="guest_ok"
            type="checkbox"
            {...register("guest_ok")}
            className="checkbox checkbox-primary checkbox-sm"
            defaultChecked={config?.guest_ok}
          />
          <span className="text-sm">Allow guest access</span>
        </label>
        <label className="flex items-center space-x-2 cursor-pointer">
          <input
            id="read_only"
            type="checkbox"
            {...register("read_only")}
            className="checkbox checkbox-primary checkbox-sm"
            defaultChecked={config?.read_only}
          />
          <span className="text-sm">Read only</span>
        </label>
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
