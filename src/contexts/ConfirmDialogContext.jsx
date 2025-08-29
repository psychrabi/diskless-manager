import React, { useCallback, useRef, useState } from 'react';
import { Modal, Button } from '@/components/ui';
import { ConfirmDialogContext } from './confirmDialog';

export const ConfirmDialogProvider = ({ children }) => {
  const [isOpen, setIsOpen] = useState(false);
  const [options, setOptions] = useState({
    title: 'Confirm',
    description: '',
    content: null,
    confirmText: 'Confirm',
    cancelText: 'Cancel',
    confirmVariant: 'primary',
    size: 'md',
  });

  const resolverRef = useRef(null);

  const confirm = useCallback((opts = {}) => {
    return new Promise((resolve) => {
      resolverRef.current = resolve;
      setOptions((prev) => ({ ...prev, ...opts }));
      setIsOpen(true);
    });
  }, []);

  const close = useCallback(() => {
    setIsOpen(false);
    // Clear resolver after closing to avoid memory leaks
    resolverRef.current = null;
  }, []);

  const handleCancel = useCallback(() => {
    if (resolverRef.current) resolverRef.current(false);
    close();
  }, [close]);

  const handleConfirm = useCallback(() => {
    if (resolverRef.current) resolverRef.current(true);
    close();
  }, [close]);

  return (
    <ConfirmDialogContext.Provider value={{ confirm }}>
      {children}
      <Modal isOpen={isOpen} onClose={handleCancel} title={options.title} size={options.size}>
        <div className="space-y-4">
          {options.description && <p>{options.description}</p>}
          {options.content}
          <div className="flex justify-end space-x-3">
            <Button variant={options.confirmVariant || 'primary'} onClick={handleConfirm}>
              {options.confirmText || 'Confirm'}
            </Button>
            <Button variant="destructive" onClick={handleCancel}>
              {options.cancelText || 'Cancel'}
            </Button>
          </div>
        </div>
      </Modal>
    </ConfirmDialogContext.Provider>
  );
};


