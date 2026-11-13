import { Controller } from "react-hook-form";
import { useState } from "react";
import { Button, Input } from "../../ui";
import { getRetentionPlan, runRetention } from "@/api/modules/system";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
import { useConfirm } from "@/contexts/confirmDialog";

/**
 * Prewarmed replacement writebacks.
 *
 * A spare is a job-owned clone that is only adopted through the journaled
 * replacement path after every ownership and session check passes, so these
 * limits never affect a client's active disk.
 */
function StoragePrewarmForm({ control, register, errors, config }) {
  const prewarm = config?.spare_prewarm || {};
  const retention = config?.retention || {};
  const maintenance = config?.maintenance || {};
  const [plan, setPlan] = useState(null);
  const [planError, setPlanError] = useState("");
  const [previewing, setPreviewing] = useState(false);
  const [executing, setExecuting] = useState(false);
  const confirm = useConfirm();

  const preview = async () => {
    setPreviewing(true);
    setPlanError("");
    try {
      setPlan(await getRetentionPlan());
    } catch (err) {
      setPlan(null);
      setPlanError(err?.message || String(err));
    } finally {
      setPreviewing(false);
    }
  };

  const execute = async () => {
    const confirmed = await confirm({
      title: "Delete eligible image snapshots",
      description: `Delete every image snapshot that falls outside the retention policy? This cannot be undone, and clients restored from those snapshots lose that path back.`,
      confirmText: "Delete snapshots",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });
    if (!confirmed) return;
    setExecuting(true);
    setPlanError("");
    try {
      setPlan(await runRetention(true));
    } catch (err) {
      setPlanError(err?.message || String(err));
    } finally {
      setExecuting(false);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <Field orientation="horizontal">
        <Controller
          name="spare_prewarm.enabled"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="spare-prewarm-enabled"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <FieldContent>
          <FieldLabel htmlFor="spare-prewarm-enabled" className="cursor-pointer">
            Prewarm replacement writebacks
          </FieldLabel>
          <FieldDescription>
            Build a clean replacement clone for offline non-persistent clients so the next boot
            skips clone construction. Disabled by default.
          </FieldDescription>
        </FieldContent>
      </Field>

      <Input
        id="spare-prewarm-limit"
        type="number"
        label="Maximum ready spares"
        register={register("spare_prewarm.max_ready_spares")}
        defaultValue={prewarm.max_ready_spares ?? 2}
        min={0}
        max={64}
        error={
          errors.spare_prewarm?.max_ready_spares?.message ||
          (errors.spare_prewarm?.max_ready_spares
            ? "Enter a whole number from 0 to 64."
            : undefined)
        }
        helperText="Pool-wide cap on ready spares. 0 keeps the feature on without building new spares."
      />

      <Input
        id="spare-prewarm-ttl"
        type="number"
        label="Discard unused spares after (hours)"
        register={register("spare_prewarm.ttl_hours")}
        defaultValue={prewarm.ttl_hours ?? 24}
        min={1}
        max={168}
        error={
          errors.spare_prewarm?.ttl_hours?.message ||
          (errors.spare_prewarm?.ttl_hours ? "Enter a whole number from 1 to 168." : undefined)
        }
        helperText="Age limit for spares that were never consumed."
      />

      <Controller
        name="spare_prewarm.min_free_bytes"
        control={control}
        defaultValue={prewarm.min_free_bytes ?? 50 * 1024 ** 3}
        render={({ field }) => (
          <Input
            id="spare-prewarm-free"
            type="number"
            label="Minimum free pool space (GiB)"
            value={Math.round((field.value ?? 0) / 1024 ** 3)}
            onChange={(event) =>
              field.onChange(Math.round(Number(event.target.value || 0) * 1024 ** 3))
            }
            onBlur={field.onBlur}
            min={0}
            error={
              errors.spare_prewarm?.min_free_bytes?.message ||
              (errors.spare_prewarm?.min_free_bytes
                ? "Enter a whole number of GiB."
                : undefined)
            }        helperText="New spares are only built while the pool has at least this much free space."
        />
      )}
    />

      <div className="mt-2 flex flex-col gap-3 border-t pt-3">
        <div>
          <h3 className="text-sm font-medium">Image retention</h3>
          <p className="text-xs text-muted-foreground">
            Remove old image snapshots. Planning is read-only; a destructive run only
            happens after the confirmation prompt and never touches a snapshot a client, clone,
            spare or session still references.
          </p>
        </div>

        <Field orientation="horizontal">
          <Controller
            name="retention.enabled"
            control={control}
            defaultValue={false}
            render={({ field }) => (
              <Checkbox
                id="retention-enabled"
                name={field.name}
                ref={field.ref}
                checked={Boolean(field.value)}
                onCheckedChange={field.onChange}
                onBlur={field.onBlur}
              />
            )}
          />
          <FieldContent>
            <FieldLabel htmlFor="retention-enabled" className="cursor-pointer">
              Enable image retention
            </FieldLabel>
            <FieldDescription>Disabled by default.</FieldDescription>
          </FieldContent>
        </Field>

        <Field orientation="horizontal">
          <Controller
            name="retention.dry_run"
            control={control}
            defaultValue
            render={({ field }) => (
              <Checkbox
                id="retention-dry-run"
                name={field.name}
                ref={field.ref}
                checked={Boolean(field.value)}
                onCheckedChange={field.onChange}
                onBlur={field.onBlur}
              />
            )}
          />
          <FieldContent>
            <FieldLabel htmlFor="retention-dry-run" className="cursor-pointer">
              Dry run only
            </FieldLabel>
            <FieldDescription>Record the plan without deleting any snapshot.</FieldDescription>
          </FieldContent>
        </Field>

        <Input
          id="retention-keep"
          type="number"
          label="Always keep newest snapshots"
          register={register("retention.min_keep_snapshots")}
          defaultValue={retention.min_keep_snapshots ?? 3}
          min={0}
          max={100}
          error={
            errors.retention?.min_keep_snapshots?.message ||
            (errors.retention?.min_keep_snapshots
              ? "Enter a whole number from 0 to 100."
              : undefined)
          }
          helperText="Per image, before the minimum age is applied."
        />

        <Input
          id="retention-age"
          type="number"
          label="Minimum snapshot age (hours)"
          register={register("retention.min_age_hours")}
          defaultValue={retention.min_age_hours ?? 168}
          min={1}
          max={8760}
          error={
            errors.retention?.min_age_hours?.message ||
            (errors.retention?.min_age_hours
              ? "Enter a whole number from 1 to 8760."
              : undefined)
          }
          helperText="Snapshots younger than this are never proposed."
        />

        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" variant="outline" loading={previewing} onClick={preview}>
            Preview retention
          </Button>
          {plan && plan.dry_run === false && (
            <Button
              type="button"
              variant="destructive"
              loading={executing}
              onClick={execute}
            >
              Run retention
            </Button>
          )}
        </div>

        {planError && (
          <p role="alert" className="text-sm text-destructive">
            {planError}
          </p>
        )}

        {plan && (
          <div className="flex flex-col gap-1 text-xs text-muted-foreground" data-testid="retention-plan">
            <p>
              {plan.candidates.length} eligible · {plan.skipped.length} kept
              {plan.executed === false ? " · dry run" : ` · ${plan.deleted ?? 0} deleted`}
            </p>
            {plan.candidates.slice(0, 5).map((candidate) => (
              <p key={candidate.snapshot_id}>{candidate.full_name}</p>
            ))}
            {plan.failures?.slice(0, 5).map((failure) => (
              <p key={failure} className="text-destructive">
                {failure}
              </p>
            ))}
          </div>
        )}
      </div>

      <div className="mt-2 flex flex-col gap-3 border-t pt-3">
        <div>
          <h3 className="text-sm font-medium">Scheduled maintenance</h3>
          <p className="text-xs text-muted-foreground">
            Run ZFS scrub and TRIM inside an off-hours window. Manual runs are available from
            the dashboard card and never start while another scan is active.
          </p>
        </div>

        <Field orientation="horizontal">
          <Controller
            name="maintenance.enabled"
            control={control}
            defaultValue={false}
            render={({ field }) => (
              <Checkbox
                id="maintenance-enabled"
                name={field.name}
                ref={field.ref}
                checked={Boolean(field.value)}
                onCheckedChange={field.onChange}
                onBlur={field.onBlur}
              />
            )}
          />
          <FieldContent>
            <FieldLabel htmlFor="maintenance-enabled" className="cursor-pointer">
              Schedule scrub and TRIM
            </FieldLabel>
            <FieldDescription>Disabled by default.</FieldDescription>
          </FieldContent>
        </Field>

        <Input
          id="scrub-interval"
          type="number"
          label="Scrub interval (days)"
          register={register("maintenance.scrub_interval_days")}
          defaultValue={maintenance.scrub_interval_days ?? 30}
          min={1}
          max={365}
          error={
            errors.maintenance?.scrub_interval_days?.message ||
            (errors.maintenance?.scrub_interval_days
              ? "Enter a whole number from 1 to 365."
              : undefined)
          }
          helperText="Automatic scrubs only run inside the window below."
        />

        <Input
          id="trim-interval"
          type="number"
          label="TRIM interval (days)"
          register={register("maintenance.trim_interval_days")}
          defaultValue={maintenance.trim_interval_days ?? 7}
          min={1}
          max={365}
          error={
            errors.maintenance?.trim_interval_days?.message ||
            (errors.maintenance?.trim_interval_days
              ? "Enter a whole number from 1 to 365."
              : undefined)
          }
          helperText="TRIM requires a drive that supports discard."
        />

        <div className="grid grid-cols-2 gap-3">
          <Input
            id="maintenance-window-start"
            type="number"
            label="Window start (hour)"
            register={register("maintenance.window_start_hour")}
            defaultValue={maintenance.window_start_hour ?? 2}
            min={0}
            max={23}
            helperText="Inclusive, 0-23."
          />
          <Input
            id="maintenance-window-end"
            type="number"
            label="Window end (hour)"
            register={register("maintenance.window_end_hour")}
            defaultValue={maintenance.window_end_hour ?? 5}
            min={0}
            max={23}
            helperText="Exclusive; may wrap past midnight."
          />
        </div>
      </div>
    </div>
  );
}

export default StoragePrewarmForm;
