import { Input } from "../../ui";

const ISCSIForm = ({ register, errors, config }) => {
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
        <span className="ml-2">ISCSI Server (Start at boot)</span>
      </label>
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
