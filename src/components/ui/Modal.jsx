import React, { useRef, useEffect } from 'react';
import { X } from 'lucide-react';
import { Button } from './Button.jsx';

export const Modal = ({ isOpen, onClose, title, children, size = 'md', id = 'modal-dialog' }) => {
  const dialogRef = useRef(null);

  useEffect(() => {
    if (isOpen && dialogRef.current) {
      dialogRef.current.showModal();
    } else if (!isOpen && dialogRef.current) {
      dialogRef.current.close();
    }
  }, [isOpen]);

  const sizeClasses = {
    sm: 'modal-sm',
    md: '',
    lg: 'modal-lg',
    xl: 'modal-xl',
    '2xl': 'modal-2xl',
    '3xl': 'modal-3xl',
    full: 'modal-bottom sm:modal-middle',
  };

  return (
    <dialog ref={dialogRef} id={id} className={`modal ${isOpen ? 'modal-open' : ''}`} tabIndex={0} onClose={onClose}>
      <div className={`modal-box ${sizeClasses[size] || ''}`}>
        <div className='flex justify-between items-center mb-4 border-b border-base-200 pb-3'>
          <h2 className='text-xl font-semibold'>{title}</h2>
          <Button onClick={onClose} variant='ghost' size='icon' className='h-8 w-8'>
            <X className='h-5 w-5' />
          </Button>
        </div>
        <div>{children}</div>
      </div>
      <form method='dialog' className='modal-backdrop'>
        <button tabIndex={-1} aria-label='Close' onClick={onClose}>close</button>
      </form>
    </dialog>
  );
};
