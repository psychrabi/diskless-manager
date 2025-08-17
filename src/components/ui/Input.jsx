export const Input = ({ label, id, register, placeholder, type = 'text', required = false, className = '', title }) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend htmlFor={id} className='fieldset-legend'>{label}</legend>}
    <input type={type} id={id} {...register} placeholder={placeholder} title={title} required={required} className='input w-full' />
  </fieldset>
);