export const Select = ({
  label,
  id,
  register,
  value,
  onChange,
  children,
  className = "",
  required = false,
  disabled = false,
}) => (
  <fieldset className={`fieldset ${className}`}>
    {label && <legend className="fieldset-legend">{label}</legend>}
    <select
      id={id}
      {...register}
      defaultValue={value}
      onChange={onChange}
      className="select w-full"
      required={required}
      disabled={disabled}
    >
      {children}
    </select>
  </fieldset>
);
