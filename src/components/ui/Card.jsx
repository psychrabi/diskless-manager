import React from "react";
import PropTypes from 'prop-types';

export const Card = ({ title, subtitle = "", icon, children, className = '', titleClassName = '', actions, bodyClass = '', headerClass = 'pb-0' }) => (
  <div className={`card bg-base-100 shadow-xl ${className}`}>
    {title && (
      <div className={`card-body ${headerClass}`}>
        <div className='flex justify-between items-center'>
          <div className='flex items-center min-w-0'>
            {icon && React.createElement(icon, { className: 'h-5 w-5 md:h-6 md:w-6 mr-3 text-primary flex-shrink-0' })}
            <div className='flex flex-col'>
              <h3 className={`card-title text-lg md:text-xl font-semibold truncate ${titleClassName}`}>{title}</h3>
              {subtitle ? <h4 className="text-base-content/70 truncate">{subtitle}</h4> : null}
            </div>
          </div>
          {actions && <div className='flex space-x-2 flex-shrink-0'>{actions}</div>}
        </div>
      </div>
    )}
    <div className={`card-body ${bodyClass}`}>{children}</div>
  </div>
);

Card.propTypes = {
  title: PropTypes.string,
  icon: PropTypes.elementType,
  children: PropTypes.node,
  className: PropTypes.string,
  titleClassName: PropTypes.string,
  actions: PropTypes.node,
  bodyClass: PropTypes.string,
  headerClass: PropTypes.string,
};