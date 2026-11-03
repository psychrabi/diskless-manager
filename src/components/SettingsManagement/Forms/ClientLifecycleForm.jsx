import { Input } from "../../ui";
const ClientLifecycleForm = ({ register, errors, config }) => (
  <div className="grid gap-4">
    <Input
      id="offline-reset-delay"
      type="number"
      label="Delete non-persistent client storage after (minutes offline)"
      register={register("non_persistent_reset_delay_minutes")}
      defaultValue={config?.non_persistent_reset_delay_minutes ?? 5}
      min={1}
      max={1440}
      error={
        errors.non_persistent_reset_delay_minutes?.message ||
        (errors.non_persistent_reset_delay_minutes
          ? "Enter a whole number from 1 to 1440."
          : undefined)
      }
      helperText="Applies to all non-persistent clients. The timer starts when the disk connection ends; reconnecting cancels deletion."
    />
    <Input
      id="boot-handoff-window"
      type="number"
      label="Boot handoff window (seconds)"
      register={register("boot_handoff_seconds")}
      defaultValue={config?.boot_handoff_seconds ?? 300}
      min={30}
      max={900}
      error={
        errors.boot_handoff_seconds?.message ||
        (errors.boot_handoff_seconds
          ? "Enter a whole number from 30 to 900."
          : undefined)
      }
      helperText="How long a boot may take to open its own disk session after iPXE. The reset timer stays paused during this window; a boot that never connects is reported as a boot failure."
    />
  </div>
);
export default ClientLifecycleForm;
