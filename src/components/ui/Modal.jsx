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
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
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
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
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
    md: "",
    lg: "max-w-lg",
    xl: "max-w-xl",
    "2xl": "max-w-2xl",
    "3xl": "max-w-3xl",
    "4xl": "max-w-4xl",
    "5xl": "max-w-5xl",
    full: "modal-bottom sm:modal-middle",
  };

  return (
    <dialog
      ref={dialogRef}
      id={id}
      className={`modal ${isOpen ? "modal-open" : ""}`}
      onClose={onClose}
      onKeyDown={handleKeyDown}
      aria-modal="true"
      aria-labelledby={`${id}-title`}
    >
      <div className={`modal-box ${sizeClasses[size] || ""}`}>
        <div className="flex justify-between items-center mb-4 border-b border-base-200 pb-3">
          <h2 id={`${id}-title`} className="text-xl font-semibold">
            {title}
          </h2>
          <Button
            onClick={onClose}
            variant="destructive"
            size="icon"
            className="h-8 w-8"
            aria-label="Close modal"
            title="Close modal"
          >
            <X className="h-5 w-5" aria-hidden="true" />
          </Button>
        </div>
        <div>{children}</div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button tabIndex={-1} aria-label="Close modal" onClick={onClose}>
          close
        </button>
      </form>
    </dialog>
  );
};
