import { X } from "lucide-react";
import { useEffect, useRef } from "react";
import { Button } from "./Button.jsx";

export const Modal = ({
  isOpen,
  onClose,
  title,
  children,
  size = "md",
  id = "modal-dialog",
  showCloseButton = true,
  className = "",
}) => {
  const dialogRef = useRef(null);
  const lastFocusedElement = useRef(null);

  useEffect(() => {
    if (isOpen && dialogRef.current) {
      // Store the last focused element
      lastFocusedElement.current = document.activeElement;

      dialogRef.current.showModal();

      // Focus the first focusable element in the modal
      const focusableElements = dialogRef.current.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      if (focusableElements.length > 0) {
        focusableElements[0].focus();
      }
    } else if (!isOpen && dialogRef.current) {
      dialogRef.current.close();

      // Restore focus to the element that opened the modal
      if (lastFocusedElement.current) {
        lastFocusedElement.current.focus();
      }
    }
  }, [isOpen]);

  // Handle keyboard navigation
  const handleKeyDown = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
      return;
    }

    // Trap focus inside modal
    if (e.key === "Tab") {
      const focusableElements = dialogRef.current?.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );

      if (!focusableElements || focusableElements.length === 0) return;

      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];

      if (e.shiftKey && document.activeElement === firstElement) {
        e.preventDefault();
        lastElement.focus();
      } else if (!e.shiftKey && document.activeElement === lastElement) {
        e.preventDefault();
        firstElement.focus();
      }
    }
  };

  const sizeClasses = {
    sm: "max-w-sm",
    md: "max-w-md",
    lg: "max-w-lg",
    xl: "max-w-xl",
    "2xl": "max-w-2xl",
    "3xl": "max-w-3xl",
    "4xl": "max-w-4xl",
    "5xl": "max-w-5xl",
    full: "max-w-full mx-4",
  };

  return (
    <dialog
      ref={dialogRef}
      id={id}
      className="modal modal-professional"
      onKeyDown={handleKeyDown}
      onClose={onClose}
    >
      <div className={`modal-box ${sizeClasses[size]} ${className}`}>
        {/* Header */}
        {(title || showCloseButton) && (
          <div className="flex items-center justify-between mb-6 pb-4 border-b border-base-200/30">
            {title && (
              <h2 className="text-heading-lg font-semibold text-base-content">
                {title}
              </h2>
            )}
            {showCloseButton && (
              <Button
                variant="ghost"
                size="icon"
                onClick={onClose}
                className="ml-auto hover:bg-base-200"
                aria-label="Close modal"
              >
                <X className="h-4 w-4" />
              </Button>
            )}
          </div>
        )}

        {/* Content */}
        <div className="modal-content">
          {children}
        </div>
      </div>

      {/* Backdrop */}
      <form method="dialog" className="modal-backdrop bg-black/20 backdrop-blur-sm">
        <button onClick={onClose} aria-label="Close modal">close</button>
      </form>
    </dialog>
  );
};
