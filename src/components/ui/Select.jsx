import PropTypes from 'prop-types';

export const Select = ({ label, id, register, value, onChange, children, className = '', required = false, disabled = false }) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend className='fieldset-legend'>{label}</legend>}
    <select id={id} {...register} defaultValue={value} onChange={onChange} className='select w-full' required={required} disabled={disabled}>
      {children}
    </select>
  </fieldset>
);

Select.propTypes = {
  label: PropTypes.string,
  id: PropTypes.string,
  register: PropTypes.object,
  value: PropTypes.string,
  onChange: PropTypes.func,
  children: PropTypes.node,
  className: PropTypes.string,
  required: PropTypes.bool,
  disabled: PropTypes.bool,
};
