import PropTypes from 'prop-types';

export const Input = ({ label, id, register, placeholder, type = 'text', required = false, className = '', title, disabled = false, error }) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend className='fieldset-legend'>{label}</legend>}
    <input
      type={type}
      id={id}
      {...register}
      placeholder={placeholder}
      title={title}
      required={required}
      className='input w-full'
      disabled={disabled}
      aria-invalid={!!error}
      aria-describedby={error ? `${id}-error` : undefined}
    />
    {error && <span id={`${id}-error`} role="alert" className="text-error text-sm">{error}</span>}
  </fieldset>
);

Input.propTypes = {
  label: PropTypes.string,
  id: PropTypes.string.isRequired,
  register: PropTypes.object,
  placeholder: PropTypes.string,
  type: PropTypes.string,
  required: PropTypes.bool,
  className: PropTypes.string,
  title: PropTypes.string,
  disabled: PropTypes.bool,
  error: PropTypes.string,
};