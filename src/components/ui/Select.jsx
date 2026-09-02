import { cn } from "@/lib/utils";

export const Select = ({
  label,
  id,
  register,
  value,
  onChange,
  children,
  className = "",
  required = false,
  disabled = false,
  error,
  helperText,
}) => {
  const { onChange: regOnChange, ...regRest } = register || {};

  return (
    <fieldset className={cn("fieldset", className)}>
      {label && <legend className="fieldset-legend">{label}</legend>}
      <select
        id={id}
        {...regRest}
        defaultValue={value}
        onChange={(e) => {
          if (regOnChange) regOnChange(e);
          if (onChange) onChange(e);
        }}
        className={`select w-full ${error ? "select-error" : ""}`}
        required={required}
        disabled={disabled}
        aria-invalid={!!error}
        aria-describedby={
          error ? `${id}-error` : helperText ? `${id}-helper` : undefined
        }
      >
        {children}
      </select>
      {error && (
        <span id={`${id}-error`} role="alert" className="form-error">
          {error}
        </span>
      )}
      {helperText && !error && (
        <span id={`${id}-helper`} className="form-helper">
          {helperText}
        </span>
      )}
    </fieldset>
  );
};
