export const Input = ({ label, id, value, onChange, placeholder, type = 'text', required = false, className = '', pattern = '', title }) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend htmlFor={id} className='fieldset-legend'>{label}</legend>}
    <input
      type={type}
      id={id}
      value={value}
      onChange={onChange}
      placeholder={placeholder}
      title={title}
      required={required}
      pattern={pattern}
      className='input w-full'
    />
  </fieldset>
);
