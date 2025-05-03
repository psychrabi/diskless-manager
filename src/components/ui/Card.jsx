import React from 'react';

export const Card = ({ title, icon, children, className = '', titleClassName = '' }) => (
  <div className={`bg-white dark:bg-gray-800 shadow-lg rounded-lg p-4 md:p-6 border border-gray-200 dark:border-gray-700 ${className}`}>
    {title && (
      <div className="flex items-center mb-4">
        {icon && React.createElement(icon, { className: "h-5 w-5 md:h-6 md:w-6 mr-3 text-blue-600 dark:text-blue-400 flex-shrink-0" })}
        <h3 className={`text-lg md:text-xl font-semibold text-gray-800 dark:text-gray-200 ${titleClassName}`}>{title}</h3>
      </div>
    )}
    <div>{children}</div>
  </div>
);
