import React from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Modal } from './Modal';
import { Button } from './Button';
import { useNotification } from '@/contexts/notification';

const FormModal = ({
  isOpen,
  setIsOpen,
  title,
  schema,
  defaultValues,
  onSubmit: parentOnSubmit,
  children,
  submitButtonText = 'Submit',
  refresh,
}) => {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
    ...rest
  } = useForm({
    resolver: zodResolver(schema),
    defaultValues,
  });

  const onSubmit = async (data) => {
    try {
      await parentOnSubmit(data, showNotification);
      setIsOpen(false);
      reset();
      refresh && refresh();
    } catch (error) {
      showNotification('error', 'Operation Failed', error.message || 'An unknown error occurred');
    }
  };

  const handleClose = () => {
    setIsOpen(false);
    reset();
  };

  return (
    <Modal isOpen={isOpen} onClose={handleClose} title={title}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        {typeof children === 'function'
          ? children({ register, errors, isSubmitting, reset, ...rest })
          : children}
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" disabled={isSubmitting}>
            {isSubmitting ? 'Processing...' : submitButtonText}
          </Button>
          <Button type="button" variant="destructive" onClick={handleClose}>
            Cancel
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export { FormModal };