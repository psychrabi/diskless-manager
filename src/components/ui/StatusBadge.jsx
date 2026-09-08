import { AlertCircle, CheckCircle2, Clock, Minus, XCircle } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";

export const StatusBadge = ({
  status,
  children,
  size = "md",
  showIcon = true,
  className = ""
}) => {
  const statusConfig = {
    success: { variant: "outline", icon: CheckCircle2, label: children || "Success" },
    error: { variant: "destructive", icon: XCircle, label: children || "Error" },
    warning: { variant: "outline", icon: AlertCircle, label: children || "Warning" },
    info: { variant: "outline", icon: Clock, label: children || "Info" },
    neutral: { variant: "secondary", icon: Minus, label: children || "Neutral" },
    running: { variant: "outline", icon: CheckCircle2, label: children || "Running" },
    stopped: { variant: "destructive", icon: XCircle, label: children || "Stopped" },
    pending: { variant: "outline", icon: Clock, label: children || "Pending" },
  };

  const sizeClasses = {
    sm: "h-5 px-2 text-xs",
    md: "h-5.5 px-2.5 text-xs",
    lg: "h-6 px-3 text-sm",
  };

  const config = statusConfig[status] || statusConfig.neutral;
  const Icon = config.icon;

  return (
    <Badge
      variant={config.variant}
      className={cn(sizeClasses[size], className)}
    >
      {showIcon && <Icon data-icon="inline-start" />}
      {config.label}
    </Badge>
  );
};