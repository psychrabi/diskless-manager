import { Input } from "../../ui";

const WindowsServicingForm = ({ register }) => (
  <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
    <p className="text-sm text-muted-foreground md:col-span-2">
      Configure the dedicated Windows worker for automatic driver preparation. SSH uses the Diskless Manager service account’s key and verified known-hosts entry; passwords are not stored here.
    </p>
    <Input
      label="Windows worker host"
      register={register("worker_host")}
      placeholder="192.168.1.50"
      autoComplete="off"
    />
    <Input
      label="SSH username"
      register={register("worker_username")}
      placeholder="Administrator"
      autoComplete="username"
    />
    <Input
      className="md:col-span-2"
      label="Helper executable"
      register={register("helper_executable_path")}
      placeholder={String.raw`C:\Program Files\Diskless Manager\diskless-windows-servicer.exe`}
      autoComplete="off"
    />
    <Input
      label="Server iSCSI address"
      register={register("iscsi_portal")}
      placeholder="192.168.1.250"
      autoComplete="off"
    />
    <Input
      label="Report server URL"
      register={register("report_server_url")}
      placeholder="http://192.168.1.250:8080"
      autoComplete="url"
    />
    <label className="flex flex-col gap-1 text-sm">
      <span className="font-medium">Windows NIC installation strategy</span>
      <select className="rounded-md border bg-background px-3 py-2" {...register("driver_install_strategy")}>
        <option value="offline_dism">Offline DISM (current)</option>
        <option value="nexboot_boot_time">NexBoot boot-time adaptation (test)</option>
      </select>
      <span className="text-xs text-muted-foreground">
        NexBoot stages the bundle and installs drivers with pnputil as SYSTEM on the next client boot.
      </span>
    </label>
  </div>
);

export default WindowsServicingForm;
