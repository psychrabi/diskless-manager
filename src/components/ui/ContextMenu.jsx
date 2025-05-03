import React, { useRef, useCallback, useEffect } from 'react';

export const ContextMenu = ({ isOpen, xPos, yPos, targetClient, onClose, actions }) => {
  const menuRef = useRef(null);

  const handleClickOutside = useCallback((event) => {
    if (menuRef.current && !menuRef.current.contains(event.target)) {
      onClose();
    }
  }, [onClose]);

  useEffect(() => {
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      document.addEventListener('touchstart', handleClickOutside);
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('touchstart', handleClickOutside);
    };
  }, [isOpen, handleClickOutside]);

  if (!isOpen) return null;

  return (
    <div
      ref={menuRef}
      style={{
        position: 'fixed',
        top: yPos,
        left: xPos,
        zIndex: 1000,
      }}
      className="bg-white dark:bg-gray-800 rounded-md shadow-lg w-48 py-1 ring-1 ring-black ring-opacity-5 focus:outline-none"
    >
      {actions.map((action) => (
        <button
          key={action.label}
          onClick={() => {
            action.onClick(targetClient);
            onClose();
          }}
          className="w-full text-left px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
          {action.label}
        </button>
      ))}
    </div>
  );
};
