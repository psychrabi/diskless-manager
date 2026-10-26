import { Input } from "../../ui";
const ClientLifecycleForm = ({ register, errors, config }) => (
  <div>
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
  </div>
);
export default ClientLifecycleForm;
