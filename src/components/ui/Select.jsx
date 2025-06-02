export const Select = ({ label, id, value, onChange, children, className = '', required = false }) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend htmlFor={id} className='fieldset-legend'>{label}</legend>}
    <select id={id} defaultValue={value} onChange={onChange} className='select w-full' required={required} >
      {children}
    </select>
  </fieldset>
);