import { createElement } from "react";
import { cn } from "@/lib/utils";
import {
  Card as ShadcnCard,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

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
  const variantClasses = {
    default: "",
    elevated: "shadow-xl",
    outlined: "ring-2",
    ghost: "bg-transparent shadow-none ring-0",
  };

  const sizeClasses = {
    sm: "text-sm",
    default: "",
    lg: "text-lg",
  };

  return (
    <ShadcnCard className={cn(variantClasses[variant], sizeClasses[size], className)}>
      {title && (
        <CardHeader
          className={cn("flex flex-row flex-wrap items-center justify-between gap-3", headerClass)}
        >
          <div className="flex items-center gap-2 min-w-0">
            {icon && (
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">
                {createElement(icon, { className: "size-4 text-primary" })}
              </span>
            )}
            <div className="flex flex-col gap-0.5 min-w-0">
              <CardTitle className={cn(titleClassName)}>{title}</CardTitle>
              {subtitle && <CardDescription>{subtitle}</CardDescription>}
            </div>
          </div>
          {actions && (
            <div className="flex flex-wrap items-center gap-2">{actions}</div>
          )}
        </CardHeader>
      )}
      <CardContent className={cn(bodyClass)}>{children}</CardContent>
    </ShadcnCard>
  );
};
