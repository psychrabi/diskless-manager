import React from 'react';

export const Input = ({ label, id, value, onChange, type = 'text', className = '', required = false, pattern = '' }) => (
  <div className={`space-y-1 ${className}`}>
    <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300">
      {label}
    </label>
    <input
      type={type}
      id={id}
      value={value}
      onChange={onChange}
      className="mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-gray-200 dark:focus:border-blue-400"
      required={required}
      pattern={pattern}
    />
  </div>
);
