import { useId } from "react";
import { cn } from "@/lib/utils";
import { Input as ShadcnInput } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export const Input = ({
  label,
  id: providedId,
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
  autoComplete,
  inputMode,
  ...props
}) => {
  const generatedId = useId();
  const id = providedId || generatedId;
  const errorMessage = typeof error === "string" ? error : error?.message || (error ? "This field is required" : undefined);
  const sizeClasses = {
    sm: "h-7 text-sm",
    md: "h-8",
    lg: "h-10",
  };

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label && (
        <Label htmlFor={id}>
          {label}
          {required && <span className="text-destructive ml-0.5">*</span>}
        </Label>
      )}
      <ShadcnInput
        type={type}
        id={id}
        {...register}
        {...props}
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
          sizeClasses[size],
          disabled && "cursor-not-allowed",
        )}
      />
      {error && (
        <span id={`${id}-error`} role="alert" className="text-sm text-destructive">
          {errorMessage}
        </span>
      )}
      {helperText && !error && (
        <span id={`${id}-helper`} className="text-sm text-muted-foreground">
          {helperText}
        </span>
      )}
    </div>
  );
};
