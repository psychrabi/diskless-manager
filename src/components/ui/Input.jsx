export const Input = ({ label, id, value, onChange, placeholder, type = 'text', required = false, className = '', pattern = '', title }) => (
  <div className={`form-control w-full ${className}`}>
    {label && <label htmlFor={id} className='label'><span className='label-text'>{label}</span></label>}
    <input
      type={type}
      id={id}
      value={value}
      onChange={onChange}
      placeholder={placeholder}
      title={title}
      required={required}
      pattern={pattern}
      className='input input-bordered w-full'
    />
  </div>
);
