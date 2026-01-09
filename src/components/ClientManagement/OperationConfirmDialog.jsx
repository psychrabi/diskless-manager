import React from "react";
import { AlertCircle, CheckCircle, XCircle } from "lucide-react";
import { Button, Modal } from "@/components/ui";

const OperationConfirmDialog = ({
  isOpen,
  onClose,
  onConfirm,
  title,
  description,
  operationType = "shutdown",
  clientName,
  isLoading = false,
  variant = "warning",
}) => {
  const variantConfig = {
    warning: {
      icon: AlertCircle,
      buttonVariant: "warning",
      bgColor: "bg-warning/10",
      borderColor: "border-warning/30",
      textColor: "text-warning-content",
    },
    danger: {
      icon: XCircle,
      buttonVariant: "error",
      bgColor: "bg-error/10",
      borderColor: "border-error/30",
      textColor: "text-error-content",
    },
    success: {
      icon: CheckCircle,
      buttonVariant: "success",
      bgColor: "bg-success/10",
      borderColor: "border-success/30",
      textColor: "text-success-content",
    },
  };

  const config = variantConfig[variant] || variantConfig.warning;
  const Icon = config.icon;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={title || "Confirm Operation"}
      size="md"
    >
      <div className="space-y-6">
        {/* Icon and Message */}
        <div className={`${config.bgColor} border ${config.borderColor} p-4 rounded-lg flex gap-4`}>
          <Icon className={`w-6 h-6 flex-shrink-0 ${config.textColor}`} />
          <div className="flex-1">
            <p className={`text-sm ${config.textColor}`}>
              {description || `Are you sure you want to ${operationType} "${clientName}"?`}
            </p>
          </div>
        </div>

        {/* Additional Info */}
        {clientName && (
          <div className="bg-base-200/30 p-3 rounded-lg">
            <p className="text-xs text-base-content/60">Client:</p>
            <p className="text-sm font-semibold text-base-content">{clientName}</p>
          </div>
        )}

        {/* Action Buttons */}
        <div className="flex justify-end gap-3 pt-4 border-t border-base-200/30">
          <Button
            type="button"
            variant="ghost"
            onClick={onClose}
            disabled={isLoading}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant={config.buttonVariant}
            onClick={onConfirm}
            disabled={isLoading}
          >
            {isLoading ? "Processing..." : "Confirm"}
          </Button>
        </div>
      </div>
    </Modal>
  );
};

export default OperationConfirmDialog;
