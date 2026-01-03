import { Button, Modal } from "@/components/ui";
import { useCallback, useRef, useState } from "react";
import { ConfirmDialogContext } from "./confirmDialog";

export const ConfirmDialogProvider = ({ children }) => {
  const [isOpen, setIsOpen] = useState(false);
  const [inputValue, setInputValue] = useState("");
  const [options, setOptions] = useState({
    title: "Confirm",
    description: "",
    content: null,
    confirmText: "Confirm",
    cancelText: "Cancel",
    confirmVariant: "primary",
    size: "md",
    showInput: false,
    inputPlaceholder: "",
    inputLabel: "",
    inputType: "text",
  });

  const resolverRef = useRef(null);

  const confirm = useCallback((opts = {}) => {
    return new Promise((resolve) => {
      resolverRef.current = resolve;
      setOptions((prev) => ({
        ...prev,
        title: "Confirm",
        description: "",
        content: null,
        confirmText: "Confirm",
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "md",
        showInput: false,
        inputPlaceholder: "",
        inputLabel: "",
        inputType: "text",
        ...opts,
      }));
      setInputValue(opts.defaultValue || "");
      setIsOpen(true);
    });
  }, []);

  const close = useCallback(() => {
    setIsOpen(false);
    resolverRef.current = null;
  }, []);

  const handleCancel = useCallback(() => {
    if (resolverRef.current) resolverRef.current(false);
    close();
  }, [close]);

  const handleConfirm = useCallback(() => {
    if (resolverRef.current) {
      if (options.showInput) {
        resolverRef.current(inputValue);
      } else {
        resolverRef.current(true);
      }
    }
    close();
  }, [close, options.showInput, inputValue]);

  return (
    <ConfirmDialogContext.Provider value={{ confirm }}>
      {children}
      <Modal
        isOpen={isOpen}
        onClose={handleCancel}
        title={options.title}
        size={options.size}
      >
        <div className="space-y-4">
          {options.description && (
            <p className="text-sm opacity-70">{options.description}</p>
          )}

          {options.showInput && (
            <div className="space-y-2">
              {options.inputLabel && (
                <label className="text-xs font-bold uppercase tracking-wider opacity-50">
                  {options.inputLabel}
                </label>
              )}
              <input
                autoFocus
                type={options.inputType || "text"}
                placeholder={options.inputPlaceholder}
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                className="input input-bordered w-full"
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleConfirm();
                  if (e.key === "Escape") handleCancel();
                }}
              />
            </div>
          )}

          {options.content}

          <div className="flex justify-end space-x-3 mt-6">
            <Button variant="ghost" onClick={handleCancel}>
              {options.cancelText || "Cancel"}
            </Button>
            <Button
              variant={options.confirmVariant || "primary"}
              onClick={handleConfirm}
            >
              {options.confirmText || "Confirm"}
            </Button>
          </div>
        </div>
      </Modal>
    </ConfirmDialogContext.Provider>
  );
};
