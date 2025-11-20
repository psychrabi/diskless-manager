import { forwardRef } from 'react';
import PropTypes from 'prop-types';

export const Button = forwardRef(({ children, onClick, variant = 'default', size = 'md', className = '', icon: Icon, disabled = false, title = '', type = 'button' }, ref) => {
  const ariaLabel = !children && title ? title : undefined;

  // daisyUI btn base
  const variantClasses = {
    default: 'btn',
    destructive: 'btn btn-error',
    outline: 'btn btn-outline',
    ghost: 'btn btn-ghost',
    link: 'btn btn-link',
    primary: 'btn btn-primary',
    secondary: 'btn btn-secondary',
    accent: 'btn btn-accent',
    info: 'btn btn-info',
    success: 'btn btn-success',
    warning: 'btn btn-warning',
  }
  const sizeClasses = {
    sm: 'btn-sm',
    md: '',
    lg: 'btn-lg',
    icon: 'btn-square',
  }
  const iconPosition = size === 'icon' ? '' : 'mr-2';

  return (
    <button
      ref={ref}
      type={type}
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
      aria-disabled={disabled}
      className={[
        variantClasses[variant] || 'btn',
        sizeClasses[size] || '',
        className
      ].join(' ')}
    >
      {Icon && <Icon className={`h-4 w-4 ${iconPosition}`} />}
      {children}
    </button>
  );
});

Button.displayName = 'Button';

Button.propTypes = {
  children: PropTypes.node,
  onClick: PropTypes.func,
  variant: PropTypes.oneOf(['default', 'destructive', 'outline', 'ghost', 'link', 'primary', 'secondary', 'accent', 'info', 'success', 'warning']),
  size: PropTypes.oneOf(['sm', 'md', 'lg', 'icon']),
  className: PropTypes.string,
  icon: PropTypes.elementType,
  disabled: PropTypes.bool,
  title: PropTypes.string,
  type: PropTypes.oneOf(['button', 'submit', 'reset']),
};