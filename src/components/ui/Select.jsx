import React from 'react';

export const Select = ({ label, id, value, onChange, children, className = '', required = false }) => (
  <div className={`form-control w-full ${className}`}>
    {label && <label htmlFor={id} className='label'><span className='label-text'>{label}</span></label>}
    <select
      id={id}
      value={value}
      onChange={onChange}
      className='select select-bordered w-full'
      required={required}
    >
      {children}
    </select>
  </div>
);
