import { Controller } from "react-hook-form";
import { Input } from "../../ui";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";

/**
 * Prewarmed replacement writebacks.
 *
 * A spare is a job-owned clone that is only adopted through the journaled
 * replacement path after every ownership and session check passes, so these
 * limits never affect a client's active disk.
 */
function StoragePrewarmForm({ control, register, errors, config }) {
  const prewarm = config?.spare_prewarm || {};
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
            }
            helperText="New spares are only built while the pool has at least this much free space."
          />
        )}
      />
    </div>
  );
}

export default StoragePrewarmForm;
