import React from "react";
export const Card = ({ title, icon, children, className = '', titleClassName = '', actions }) => ( // Added actions prop
  <div className={`bg-white dark:bg-gray-800 shadow-lg rounded-lg p-4 md:p-6 border border-gray-200 dark:border-gray-700 flex flex-col ${className}`}>
    {title && (
      <div className="flex justify-between items-center mb-4">
        <div className="flex items-center min-w-0"> {/* Ensure title area doesn't overflow */}
            {icon && React.createElement(icon, { className: "h-5 w-5 md:h-6 md:w-6 mr-3 text-blue-600 dark:text-blue-400 flex-shrink-0" })}
            <h3 className={`text-lg md:text-xl font-semibold text-gray-800 dark:text-gray-200 truncate ${titleClassName}`}>{title}</h3>
        </div>
        {actions && <div className="flex space-x-2 flex-shrink-0">{actions}</div>} {/* Render actions if provided */}
      </div>
    )}
    <div className="flex-grow">{children}</div> {/* Allow content to grow */}
  </div>
);