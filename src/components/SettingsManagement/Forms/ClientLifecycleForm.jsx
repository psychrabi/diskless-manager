import { Label } from "@/components/ui/label";
import { Input as TextInput } from "@/components/ui/input";
const ClientLifecycleForm = ({ register, errors, config }) => (
  <div>
    <Label className="flex flex-col gap-1.5" htmlFor="offline-reset-delay">
      <span className="flex items-center gap-2 text-sm font-medium">Reset non-persistent clients after (minutes offline)</span>
      <TextInput id="offline-reset-delay" type="number" min="1" max="1440"
        className="w-full"
        defaultValue={config?.non_persistent_reset_delay_minutes ?? 5}
        {...register("non_persistent_reset_delay_minutes")} />
    </Label>
    {errors.non_persistent_reset_delay_minutes && <p className="text-destructive">Enter a whole number from 1 to 1440.</p>}
    <p className="text-sm mt-2">Applies to all non-persistent clients. The timer starts when the disk connection ends; reconnecting cancels the reset.</p>
  </div>
);
export default ClientLifecycleForm;
