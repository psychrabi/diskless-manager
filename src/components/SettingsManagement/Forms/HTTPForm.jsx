import { Input } from "../../ui";

const HTTPForm = ({ register, errors, config }) => {
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
        <span className="ml-2">HTTP Server (Start at boot)</span>
      </label>
      <Input
        label="HTTP Root Directory"
        register={register("root_dir")}
        error={errors.root_dir?.message}
        placeholder="/srv/http"
      />
      <div className="grid grid-cols-2 gap-4">
        <Input
          label="Server IP"
          register={register("server_ip")}
          error={errors.server_ip?.message}
          placeholder="*"
        />
        <Input
          label="Server Port"
          register={register("port")}
          error={errors.port?.message}
          placeholder="80"
        />
      </div>
    </div>
  );
};

export default HTTPForm;
