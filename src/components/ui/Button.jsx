import { forwardRef } from "react";

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
    },
    ref
  ) => {
    const ariaLabel = !children && title ? title : undefined;

    // daisyUI btn base
    const variantClasses = {
      default: "btn",
      destructive: "btn btn-error",
      outline: "btn btn-outline",
      ghost: "btn btn-ghost",
      link: "btn btn-link",
      primary: "btn btn-primary",
      secondary: "btn btn-secondary",
      accent: "btn btn-accent",
      info: "btn btn-info",
      success: "btn btn-success",
      warning: "btn btn-warning",
    };
    const sizeClasses = {
      sm: "btn-sm",
      md: "",
      lg: "btn-lg",
      icon: "btn-square",
    };
    const iconPosition = size === "icon" ? "" : "mr-2";

    return (
      <button
        ref={ref}
        type={type}
        onClick={onClick}
        disabled={disabled || loading}
        title={title}
        aria-label={ariaLabel}
        aria-disabled={disabled || loading}
        className={[
          variantClasses[variant] || "btn",
          sizeClasses[size] || "",
          loading ? "btn-disabled" : "",
          className,
        ].join(" ")}
      >
        {loading ? (
          <span className="loading loading-spinner loading-xs mr-2"></span>
        ) : (
          Icon && <Icon className={`h-4 w-4 ${iconPosition}`} />
        )}
        {children}
      </button>
    );
  }
);

Button.displayName = "Button";
