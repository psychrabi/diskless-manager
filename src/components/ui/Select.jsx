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
}) => {
  const { onChange: regOnChange, ...regRest } = register || {};

  return (
    <fieldset className={`fieldset ${className}`}>
      {label && <legend className="fieldset-legend">{label}</legend>}
      <select
        id={id}
        {...regRest}
        defaultValue={value}
        onChange={(e) => {
          if (regOnChange) regOnChange(e);
          if (onChange) onChange(e);
        }}
        className="select w-full"
        required={required}
        disabled={disabled}
      >
        {children}
      </select>
    </fieldset>
  );
};
