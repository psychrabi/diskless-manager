import { useId } from "react";
import { cn } from "@/lib/utils";
import { NativeSelect } from "@/components/ui/native-select";
import { Label } from "@/components/ui/label";

export const Select = ({
  label,
  id: providedId,
  register,
  value,
  onChange,
  children,
  className = "",
  required = false,
  disabled = false,
  error,
  helperText,
  subtitle,
  ...props
}) => {
  const generatedId = useId();
  const id = providedId || generatedId;
  const description = helperText || subtitle;
  const errorMessage = typeof error === "string" ? error : error?.message || (error ? "This field is required" : undefined);
  const { onChange: regOnChange, ...regRest } = register || {};

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label && <Label htmlFor={id}>{label}{required && <span className="text-destructive">*</span>}</Label>}
      <NativeSelect
        id={id}
        {...regRest}
        {...props}
        value={value}
        onChange={(e) => {
          if (regOnChange) regOnChange(e);
          if (onChange) onChange(e);
        }}
        required={required}
        disabled={disabled}
        aria-invalid={!!error}
        aria-describedby={
          error ? `${id}-error` : description ? `${id}-helper` : undefined
        }
      >
        {children}
      </NativeSelect>
      {error && (
        <span id={`${id}-error`} role="alert" className="text-sm text-destructive">
          {errorMessage}
        </span>
      )}
      {description && !error && (
        <span id={`${id}-helper`} className="text-sm text-muted-foreground">
          {description}
        </span>
      )}
    </div>
  );
};
