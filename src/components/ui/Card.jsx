import React from "react";

export const Card = ({
  title,
  subtitle = "",
  icon,
  children,
  className = "",
  titleClassName = "",
  actions,
  bodyClass = "",
  headerClass = "",
  variant = "default",
  size = "default",
}) => {
  const variants = {
    default: "card-professional",
    elevated: "card-professional shadow-xl",
    outlined: "card bg-base-100 border-2 border-base-300 shadow-sm",
    ghost: "card bg-transparent border border-base-200/30",
  };

  const sizes = {
    sm: "text-sm",
    default: "",
    lg: "text-lg",
  };

  return (
    <div className={`${variants[variant]} ${sizes[size]} ${className}`}>
      {title && (
        <div className={`card-header-professional ${headerClass} pb-0`}>
          <div className="flex justify-between items-center">
            <div className="flex items-center min-w-0 flex-1">
              {icon && (
                <div className="flex-shrink-0 mr-4">
                  <div className="p-2 w-12 h-12 bg-primary/10 rounded-lg flex items-center justify-center">
                    {React.createElement(icon, {
                      className: "h-10 w-10 text-primary",
                    })}
                  </div>
                </div>
              )}
              <div className="flex flex-col min-w-0 flex-1">
                <h3 className={`text-heading-lg font-semibold text-base-content mb-1 ${titleClassName}`}>
                  {title}
                </h3>
                {subtitle && (
                  <p className="text-body-sm text-base-content/60 leading-relaxed">
                    {subtitle}
                  </p>
                )}
              </div>
            </div>
            {actions && (
              <div className="flex items-center gap-2 ml-4 flex-shrink-0">
                {actions}
              </div>
            )}
          </div>
        </div>
      )}
      <div className={`card-body-professional ${bodyClass}`}>
        {children}
      </div>
    </div>
  );
};
