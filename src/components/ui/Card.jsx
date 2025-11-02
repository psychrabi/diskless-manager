import React from "react";

export const Card = ({ title, icon, children, className = '', titleClassName = '', actions, bodyClass = '', headerClass = 'pb-0' }) => (
  <div className={`card bg-base-100 shadow-xl ${className}`}>
    {title && (
      <div className={`card-body ${headerClass}`}>
        <div className='flex justify-between items-center'>
          <div className='flex items-center min-w-0'>
            {icon && React.createElement(icon, { className: 'h-5 w-5 md:h-6 md:w-6 mr-3 text-primary flex-shrink-0' })}
            <h3 className={`card-title text-lg md:text-xl font-semibold truncate ${titleClassName}`}>{title}</h3>
          </div>
          {actions && <div className='flex space-x-2 flex-shrink-0'>{actions}</div>}
        </div>
      </div>
    )}
    <div className={`card-body ${bodyClass}`}>{children}</div>
  </div>
);