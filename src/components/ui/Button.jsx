import { forwardRef } from "react";
import { cn } from "@/lib/utils";
import { Button as ShadcnButton } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";

const variantMap = {
  default: "default",
  primary: "default",
  secondary: "secondary",
  accent: "secondary",
  info: "outline",
  success: "outline",
  warning: "outline",
  destructive: "destructive",
  outline: "outline",
  ghost: "ghost",
  link: "link",
};

const sizeMap = {
  xs: "xs",
  sm: "sm",
  md: "default",
  lg: "lg",
  xl: "lg",
  icon: "icon",
};

export const Button = forwardRef(
  (
    {
      children,
      onClick,
      variant = "default",
      size = "md",
      className = "",
      icon: Icon,
      disabled = false,
      loading = false,
      title = "",
      type = "button",
      fullWidth = false,
      ...props
    },
    ref
  ) => {
    const ariaLabel = !children && title ? title : undefined;

    return (
      <ShadcnButton
        ref={ref}
        type={type}
        onClick={onClick}
        disabled={disabled || loading}
        title={title}
        aria-label={ariaLabel}
        {...props}
        variant={variantMap[variant] || "default"}
        size={sizeMap[size] || "default"}
        className={cn(
          fullWidth && "w-full",
          size === "xl" && "px-8 text-base",
          className
        )}
      >
        {loading ? (
          <Spinner data-icon={children ? "inline-start" : undefined} />
        ) : (
          Icon && <Icon data-icon={children ? "inline-start" : undefined} />
        )}
        {children && <span className={cn(loading && "opacity-70")}>{children}</span>}
      </ShadcnButton>
    );
  }
);

Button.displayName = "Button";
