import React, { forwardRef } from 'react';

export const Button = forwardRef(({ children, onClick, variant = 'default', size = 'default', className = '', icon: Icon, disabled = false, title = '' }, ref) => {
  const baseStyle = "inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background";
  const variantStyles = {
    default: "bg-blue-600 text-white hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600",
    destructive: "bg-red-600 text-white hover:bg-red-700 dark:bg-red-500 dark:hover:bg-red-600",
    outline: "border border-gray-300 dark:border-gray-600 bg-transparent hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-800 dark:text-gray-200",
    ghost: "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-800 dark:text-gray-200",
    link: "text-blue-600 dark:text-blue-400 underline-offset-4 hover:underline",
  };
  const sizeStyles = {
    default: "h-10 py-2 px-4",
    sm: "h-9 px-3 rounded-md",
    lg: "h-11 px-8 rounded-md",
    icon: "h-7 w-7",
  };
  const iconPosition = size === 'icon' ? 'mr-0' : 'mr-2';

  return (
    <button
      ref={ref}
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`${baseStyle} ${variantStyles[variant]} ${sizeStyles[size]} ${className}`}
    >
      {Icon && <Icon className={`h-4 w-4 ${iconPosition}`} />}
      {children}
    </button>
  );
});
