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
      fullWidth = false,
    },
    ref
  ) => {
    const ariaLabel = !children && title ? title : undefined;

    // Enhanced daisyUI btn with professional styling
    const variantClasses = {
      default: "btn btn-professional",
      destructive: "btn btn-error btn-professional",
      outline: "btn btn-outline btn-professional",
      ghost: "btn btn-ghost btn-professional hover:bg-base-200",
      link: "btn btn-link btn-professional",
      primary: "btn btn-primary btn-professional",
      secondary: "btn btn-secondary btn-professional",
      accent: "btn btn-accent btn-professional",
      info: "btn btn-info btn-professional",
      success: "btn btn-success btn-professional",
      warning: "btn btn-warning btn-professional",
    };

    const sizeClasses = {
      xs: "btn-xs px-3 py-1 text-xs",
      sm: "btn-sm px-4 py-2 text-sm",
      md: "px-6 py-2.5 text-sm",
      lg: "btn-lg px-8 py-3 text-base",
      xl: "px-10 py-4 text-lg",
      icon: "btn-square w-10 h-10",
    };

    const iconSize = {
      xs: "h-3 w-3",
      sm: "h-4 w-4", 
      md: "h-4 w-4",
      lg: "h-5 w-5",
      xl: "h-6 w-6",
      icon: "h-5 w-5",
    };

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
          variantClasses[variant] || "btn btn-professional",
          sizeClasses[size] || "",
          fullWidth ? "w-full" : "",
          loading ? "btn-disabled" : "",
          disabled ? "opacity-50 cursor-not-allowed" : "",
          className,
        ].join(" ")}
      >
        {loading ? (
          <>
            <span className="loading loading-spinner loading-xs mr-2"></span>
            {children && <span className="opacity-70">{children}</span>}
          </>
        ) : (
          <>
            {Icon && (
              <Icon 
                className={`${iconSize[size]} ${children ? "mr-2" : ""} flex-shrink-0`} 
              />
            )}
            {children}
          </>
        )}
      </button>
    );
  }
);

Button.displayName = "Button";
