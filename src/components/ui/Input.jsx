import { cn } from "@/lib/utils";

export const Input = ({
  label,
  id,
  register,
  placeholder,
  type = "text",
  required = false,
  className = "",
  title,
  disabled = false,
  error,
  helperText,
  size = "md",
  variant = "default",
  autoComplete,
  inputMode,
}) => {
  const sizeClasses = {
    sm: "input-sm",
    md: "",
    lg: "input-lg",
  };

  const variantClasses = {
    default: "form-input",
    bordered: "form-input border-2",
    ghost: "input-ghost",
  };

  return (
    <div className={cn("form-group", className)}>
      {label && (
        <label htmlFor={id} className="form-label">
          {label}
          {required && <span className="text-error ml-1">*</span>}
        </label>
      )}
      <input
        type={type}
        id={id}
        {...register}
        placeholder={placeholder}
        title={title}
        required={required}
        disabled={disabled}
        autoComplete={autoComplete}
        inputMode={inputMode}
        aria-invalid={!!error}
        aria-describedby={
          error ? `${id}-error` : helperText ? `${id}-helper` : undefined
        }
        className={cn(
          variantClasses[variant],
          sizeClasses[size],
          error && "border-error! focus:border-error! focus:ring-error/20!",
          disabled && "opacity-50 cursor-not-allowed"
        )}
      />
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
    </div>
  );
};
