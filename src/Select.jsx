import React from 'react';

const Select = ({ label, id, value, onChange, children, required = false }) => (
  <div className="mb-4">
    <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{label}</label>
    <select
      id={id}
      name={id}
      value={value}
      onChange={onChange}
      required={required}
      className="block w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm focus:ring-blue-500 focus:border-blue-500 sm:text-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100"
    >
      {children}
    </select>
  </div>
);
