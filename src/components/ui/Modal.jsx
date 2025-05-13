import React from 'react';
import { X } from 'lucide-react';
import { Button } from './Button.jsx';

export const Modal = ({ isOpen, onClose, title, children, size = 'md' }) => { // Added size prop
  if (!isOpen) return null;

  const sizeClasses = {
      sm: 'max-w-sm',
      md: 'max-w-md',
      lg: 'max-w-lg',
      xl: 'max-w-xl',
      '2xl': 'max-w-2xl',
      '3xl': 'max-w-3xl',
      full: 'max-w-full h-full m-0 rounded-none', // Example for full screen
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-50 backdrop-blur-sm transition-opacity duration-300 ease-in-out">
      <div className={`bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 w-full m-4 border border-gray-200 dark:border-gray-700 transform transition-all duration-300 ease-in-out scale-95 opacity-0 animate-fade-in-scale ${sizeClasses[size]}`}>
        <div className="flex justify-between items-center mb-4 border-b border-gray-200 dark:border-gray-700 pb-3">
          <h2 className="text-xl font-semibold">{title}</h2>
          <Button onClick={onClose} variant="ghost" size="icon" className="h-8 w-8">
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div className={size === 'full' ? 'overflow-auto h-[calc(100%-80px)]' : ''}>{children}</div> {/* Adjust height for full size */}
      </div>
      <style jsx="true">{`
        @keyframes fade-in-scale {
          from { opacity: 0; transform: scale(0.95); }
          to { opacity: 1; transform: scale(1); }
        }
        .animate-fade-in-scale { animation: fade-in-scale 0.2s ease-out forwards; }
      `}</style>
    </div>
  );
};
