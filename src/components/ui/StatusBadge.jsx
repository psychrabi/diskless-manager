import { AlertCircle, CheckCircle, Clock, Minus, XCircle } from "lucide-react";
import { cn } from "@/lib/utils";

export const StatusBadge = ({
  status,
  children,
  size = "md",
  showIcon = true,
  className = ""
}) => {
  const statusConfig = {
    success: {
      className: "status-success",
      icon: CheckCircle,
      label: children || "Success"
    },
    error: {
      className: "status-error",
      icon: XCircle,
      label: children || "Error"
    },
    warning: {
      className: "status-warning",
      icon: AlertCircle,
      label: children || "Warning"
    },
    info: {
      className: "status-info",
      icon: Clock,
      label: children || "Info"
    },
    neutral: {
      className: "status-neutral",
      icon: Minus,
      label: children || "Neutral"
    },
    running: {
      className: "status-success",
      icon: CheckCircle,
      label: children || "Running"
    },
    stopped: {
      className: "status-error",
      icon: XCircle,
      label: children || "Stopped"
    },
    pending: {
      className: "status-warning",
      icon: Clock,
      label: children || "Pending"
    }
  };

  const sizeClasses = {
    sm: "text-xs px-2 py-0.5",
    md: "text-xs px-2.5 py-1",
    lg: "text-sm px-3 py-1.5"
  };

  const iconSizes = {
    sm: "h-3 w-3",
    md: "h-3.5 w-3.5",
    lg: "h-4 w-4"
  };

  const config = statusConfig[status] || statusConfig.neutral;
  const Icon = config.icon;

  return (
    <span className={cn("status-indicator", config.className, sizeClasses[size], className, "text-base-content")}>
      {showIcon && <Icon className={`${iconSizes[size]} mr-1.5`} />}
      {config.label}
    </span>
  );
};