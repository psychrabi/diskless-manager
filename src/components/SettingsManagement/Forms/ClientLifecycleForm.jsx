const ClientLifecycleForm = ({ register, errors, config }) => (
  <div>
    <label className="form-control" htmlFor="offline-reset-delay">
      <span className="label">Reset non-persistent clients after (minutes offline)</span>
      <input id="offline-reset-delay" type="number" min="1" max="1440"
        className="input input-bordered w-full"
        defaultValue={config?.non_persistent_reset_delay_minutes ?? 5}
        {...register("non_persistent_reset_delay_minutes")} />
    </label>
    {errors.non_persistent_reset_delay_minutes && <p className="text-error">Enter a whole number from 1 to 1440.</p>}
    <p className="text-sm mt-2">Applies to all non-persistent clients. The timer starts when the disk connection ends; reconnecting cancels the reset.</p>
  </div>
);
export default ClientLifecycleForm;
