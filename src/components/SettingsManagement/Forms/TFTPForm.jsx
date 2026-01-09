import { Input } from "../../ui";

const TFTPForm = ({ register, errors, config }) => {
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
        <span className="ml-2">TFTP Server (Start at boot)</span>
      </label>

      <Input
        label="TFTP Server IP"
        register={register("server_ip")}
        error={errors.server_ip?.message}
        placeholder="0.0.0.0"
      />
      <Input
        label="TFTP Server Port"
        register={register("port")}
        error={errors.port?.message}
        placeholder="69"
      />
      <Input
        label="TFTP Options"
        register={register("options")}
        error={errors.options?.message}
        placeholder="--secure"
      />

      <Input
        label="TFTP Root Directory"
        register={register("root_dir")}
        error={errors.root_dir?.message}
        placeholder="/srv/tftp"
      />
    </div>
  );
};

export default TFTPForm;
