export const Input = ({ label, id, value, onChange, placeholder, type = 'text', required = false, className = '',pattern = '', title }) => (
  <div className={`space-y-1 ${className}`}>
    <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300">
      {label}
    </label>
    <input
      type={type}
      id={id}
      value={value}
      onChange={onChange}
      placeholder={placeholder}
      title={title}
      required={required}
      className="block w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm focus:ring-blue-500 focus:border-blue-500 sm:text-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100"  
    />
  </div>
);
