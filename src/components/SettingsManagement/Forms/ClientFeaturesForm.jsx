import { Controller } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";

function ClientFeaturesForm({ control }) {
  return (
    <div className="flex flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Turning a feature off hides its client controls. Existing CHAP settings and NVMe exports stay active; turn the feature back on to manage or remove them.
      </p>
      <Field orientation="horizontal">
        <Controller
          name="enable_chap_authentication"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="enable-chap-authentication"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <FieldContent>
          <FieldLabel htmlFor="enable-chap-authentication" className="cursor-pointer">
            Enable CHAP Authentication
          </FieldLabel>
          <FieldDescription>
            Show CHAP options in client management and allow CHAP changes for clients.
          </FieldDescription>
        </FieldContent>
      </Field>

      <Field orientation="horizontal">
        <Controller
          name="enable_experimental_nvme_target"
          control={control}
          defaultValue={false}
          render={({ field }) => (
            <Checkbox
              id="enable-experimental-nvme-target"
              name={field.name}
              ref={field.ref}
              checked={Boolean(field.value)}
              onCheckedChange={field.onChange}
              onBlur={field.onBlur}
            />
          )}
        />
        <FieldContent>
          <FieldLabel htmlFor="enable-experimental-nvme-target" className="cursor-pointer">
            Enable Experimental NVMe Target
          </FieldLabel>
          <FieldDescription>
            Show and allow the experimental NVMe/TCP target controls for clients.
          </FieldDescription>
        </FieldContent>
      </Field>
    </div>
  );
}

export default ClientFeaturesForm;
