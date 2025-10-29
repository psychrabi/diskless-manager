//! Frontend Error Handler
//! 
//! Centralized error handling for the application with consistent error patterns
//! that match the Rust backend error types.

import { useNotification } from '@/contexts/notification';
import { useAuth } from '@/contexts/auth';

// ========== ERROR TYPES ==========
/**
 * Frontend error categories matching backend patterns
 */
export const ErrorTypes = {
  AUTH: 'authentication',
  VALIDATION: 'validation',
  NETWORK: 'network',
  SYSTEM: 'system',
  NOT_FOUND: 'not_found',
  PERMISSION: 'permission',
  CONFLICT: 'conflict',
  UNKNOWN: 'unknown'
};

/**
 * Error severity levels for UI handling
 */
export const ErrorSeverity = {
  LOW: 'low',
  MEDIUM: 'medium',
  HIGH: 'high',
  CRITICAL: 'critical'
};

/**
 * Frontend error structure
 */
export class AppError extends Error {
  /**
   * @param {string} message - Error message
   * @param {string} type - Error category
   * @param {string} severity - Error severity
   * @param {string} context - Where the error occurred
   * @param {Object} details - Additional error details
   */
  constructor(message, type = ErrorTypes.UNKNOWN, severity = ErrorSeverity.MEDIUM, context = '', details = {}) {
    super(message);
    this.name = 'AppError';
    this.type = type;
    this.severity = severity;
    this.context = context;
    this.details = details;
    this.timestamp = new Date().toISOString();
  }
}

/**
 * Authentication error
 */
export class AuthError extends AppError {
  constructor(message, context = 'Authentication') {
    super(message, ErrorTypes.AUTH, ErrorSeverity.HIGH, context);
  }
}

/**
 * Validation error
 */
export class ValidationError extends AppError {
  constructor(message, context = 'Validation', details = {}) {
    super(message, ErrorTypes.VALIDATION, ErrorSeverity.MEDIUM, context, details);
  }
}

/**
 * Network error
 */
export class NetworkError extends AppError {
  constructor(message, context = 'Network', details = {}) {
    super(message, ErrorTypes.NETWORK, ErrorSeverity.MEDIUM, context, details);
  }
}

// ========== ERROR CREATOR ==========
/**
 * Create standardized error objects based on various input types
 */
export function createError(errorInput, context = 'Unknown operation') {
  // If already an AppError, return as-is
  if (errorInput instanceof AppError) {
    return errorInput;
  }

  // Handle string errors
  if (typeof errorInput === 'string') {
    return new AppError(errorInput, ErrorTypes.UNKNOWN, ErrorSeverity.MEDIUM, context);
  }

  // Handle Error objects
  if (errorInput instanceof Error) {
    return new AppError(
      errorInput.message || 'Unknown error',
      getErrorTypeFromMessage(errorInput.message),
      getErrorSeverityFromMessage(errorInput.message),
      context,
      { originalError: errorInput }
    );
  }

  // Handle Tauri invoke errors
  if (errorInput && typeof errorInput === 'object' && errorInput.message) {
    return new AppError(
      errorInput.message,
      parseErrorType(errorInput.message),
      ErrorSeverity.MEDIUM,
      context,
      errorInput
    );
  }

  // Default fallback
  return new AppError(
    'An unexpected error occurred',
    ErrorTypes.UNKNOWN,
    ErrorSeverity.MEDIUM,
    context,
    { originalError: errorInput }
  );
}

// ========== ERROR ANALYZERS ==========
/**
 * Determine error type from message content
 */
function getErrorTypeFromMessage(message) {
  const msg = message.toLowerCase();
  
  if (msg.includes('authentication') || msg.includes('token') || msg.includes('unauthorized')) {
    return ErrorTypes.AUTH;
  }
  
  if (msg.includes('validation') || msg.includes('required') || msg.includes('invalid format')) {
    return ErrorTypes.VALIDATION;
  }
  
  if (msg.includes('network') || msg.includes('connection') || msg.includes('timeout')) {
    return ErrorTypes.NETWORK;
  }
  
  if (msg.includes('permission') || msg.includes('forbidden') || msg.includes('access denied')) {
    return ErrorTypes.PERMISSION;
  }
  
  if (msg.includes('not found') || msg.includes('does not exist')) {
    return ErrorTypes.NOT_FOUND;
  }
  
  if (msg.includes('conflict') || msg.includes('already exists')) {
    return ErrorTypes.CONFLICT;
  }

  return ErrorTypes.UNKNOWN;
}

/**
 * Determine error severity from message content
 */
function getErrorSeverityFromMessage(message) {
  const msg = message.toLowerCase();
  
  if (msg.includes('critical') || msg.includes('fatal') || msg.includes('system failure')) {
    return ErrorSeverity.CRITICAL;
  }
  
  if (msg.includes('authentication') || msg.includes('permission denied')) {
    return ErrorSeverity.HIGH;
  }
  
  if (msg.includes('warning') || msg.includes('minor')) {
    return ErrorSeverity.LOW;
  }

  return ErrorSeverity.MEDIUM;
}

/**
 * Parse error type from backend response
 */
function parseErrorType(message) {
  // Check for specific error patterns
  if (message.includes('Client') && message.includes('NotFound')) {
    return ErrorTypes.NOT_FOUND;
  }
  
  if (message.includes('Config') && message.includes('Error')) {
    return ErrorTypes.VALIDATION;
  }
  
  if (message.includes('Service') && message.includes('NotAvailable')) {
    return ErrorTypes.SYSTEM;
  }

  return getErrorTypeFromMessage(message);
}

// ========== ERROR HANDLER ==========
/**
 * Create centralized error handler instance
 */
export function createErrorHandler() {
  /**
   * Handle API errors with standardized processing
   * @param {Error|string|Object} error - Error from API call
   * @param {string} context - Operation context
   * @returns {AppError}
   */
  function handleApiError(error, context = 'API call') {
    const appError = createError(error, context);
    
    // Log error for debugging
    console.error('API Error:', {
      message: appError.message,
      type: appError.type,
      severity: appError.severity,
      context: appError.context,
      timestamp: appError.timestamp,
      details: appError.details
    });

    return appError;
  }

  /**
   * Handle validation errors with field-specific messages
   * @param {Object} validationErrors - Validation error details
   * @param {string} context - Operation context
   * @returns {ValidationError}
   */
  function handleValidationError(validationErrors, context = 'Validation') {
    const fields = Object.keys(validationErrors);
    const messages = fields.map(field => `${field}: ${validationErrors[field]}`).join('; ');
    
    return new ValidationError(
      `Validation failed: ${messages}`,
      context,
      { fields: validationErrors }
    );
  }

  /**
   * Handle authentication errors and trigger logout if needed
   * @param {Error|string} error - Authentication error
   * @param {Object} authContext - Auth context for logout
   * @param {Function} showNotification - Notification function
   */
  function handleAuthError(error, authContext, showNotification) {
    const authError = new AuthError(
      typeof error === 'string' ? error : error.message,
      'Authentication'
    );

    // Show user-friendly message
    showNotification?.(
      'Session expired. Please log in again.',
      'warning'
    );

    // Trigger logout
    authContext?.logout?.();

    console.error('Authentication Error:', authError);
  }

  /**
   * Get user-friendly error message
   * @param {AppError} error - Application error
   * @returns {string}
   */
  function getUserFriendlyMessage(error) {
    switch (error.type) {
      case ErrorTypes.AUTH:
        return 'Authentication failed. Please check your credentials.';
      
      case ErrorTypes.VALIDATION:
        return error.message || 'Please check your input and try again.';
      
      case ErrorTypes.NETWORK:
        return 'Network error. Please check your connection and try again.';
      
      case ErrorTypes.PERMISSION:
        return 'You do not have permission to perform this action.';
      
      case ErrorTypes.NOT_FOUND:
        return 'The requested resource was not found.';
      
      case ErrorTypes.CONFLICT:
        return 'A conflict occurred. The resource may already exist or be in use.';
      
      case ErrorTypes.SYSTEM:
        return 'System error. Please contact support if the problem persists.';
      
      default:
        return error.message || 'An unexpected error occurred.';
    }
  }

  /**
   * Check if error should trigger a notification
   * @param {AppError} error - Application error
   * @returns {boolean}
   */
  function shouldNotify(error) {
    // Don't notify for silent operations
    const silentContext = ['background_check', 'polling', 'status_update'];
    return !silentContext.some(silent => 
      error.context.toLowerCase().includes(silent)
    );
  }

  /**
   * Get error action recommendations
   * @param {AppError} error - Application error
   * @returns {Object}
   */
  function getErrorActions(error) {
    switch (error.type) {
      case ErrorTypes.AUTH:
        return {
          primary: { label: 'Log in', action: () => window.location.reload() },
          secondary: { label: 'Cancel', action: () => {} }
        };
      
      case ErrorTypes.NETWORK:
        return {
          primary: { label: 'Retry', action: () => window.location.reload() },
          secondary: { label: 'Cancel', action: () => {} }
        };
      
      case ErrorTypes.VALIDATION:
        return {
          primary: { label: 'Fix Errors', action: () => {} },
          secondary: { label: 'Cancel', action: () => {} }
        };
      
      default:
        return {
          primary: { label: 'OK', action: () => {} }
        };
    }
  }

  return {
    handleApiError,
    handleValidationError,
    handleAuthError,
    getUserFriendlyMessage,
    shouldNotify,
    getErrorActions
  };
}

// ========== ERROR BOUNDARY COMPONENT ==========
/**
 * React error boundary for catching component errors
 */
export class ErrorBoundary {
  constructor() {
    this.hasError = false;
    this.error = null;
    this.errorInfo = null;
  }

  /**
   * Catch and handle React component errors
   */
  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  /**
   * Log component error details
   */
  static componentDidCatch(error, errorInfo) {
    console.error('React Component Error:', {
      error: error.message,
      stack: error.stack,
      errorInfo: errorInfo.componentStack,
      timestamp: new Date().toISOString()
    });
  }
}

// ========== HOOK FOR ERROR HANDLING ==========
/**
 * React hook for centralized error handling
 */
export function useErrorHandler() {
  const errorHandler = createErrorHandler();
  const authContext = useAuth();
  const { showNotification } = useNotification();

  /**
   * Handle errors with automatic user feedback
   */
  const handleError = (error, context, options = {}) => {
    const appError = errorHandler.handleApiError(error, context);
    
    // Show notification if appropriate
    if (errorHandler.shouldNotify(appError) && options.silent !== true) {
      const userMessage = errorHandler.getUserFriendlyMessage(appError);
      const severity = appError.severity === ErrorSeverity.CRITICAL ? 'error' : 'warning';
      
      showNotification(userMessage, severity);
    }

    // Handle authentication errors specially
    if (appError.type === ErrorTypes.AUTH) {
      errorHandler.handleAuthError(appError, authContext, showNotification);
    }

    return appError;
  };

  /**
   * Handle validation errors with field highlighting
   */
  const handleValidationError = (validationErrors, context) => {
    const validationError = errorHandler.handleValidationError(validationErrors, context);
    handleError(validationError, context);
    return validationError;
  };

  /**
   * Create user-friendly error messages
   */
  const createUserMessage = (error, context) => {
    const appError = createError(error, context);
    return errorHandler.getUserFriendlyMessage(appError);
  };

  return {
    handleError,
    handleValidationError,
    createUserMessage,
    getErrorActions: (error) => errorHandler.getErrorActions(createError(error, 'Unknown'))
  };
}

// ========== GLOBAL ERROR HANDLER ==========
/**
 * Set up global error handling for uncaught errors
 */
export function setupGlobalErrorHandling() {
  // Handle unhandled promise rejections
  window.addEventListener('unhandledrejection', (event) => {
    console.error('Unhandled Promise Rejection:', {
      reason: event.reason,
      promise: event.promise,
      timestamp: new Date().toISOString()
    });
    
    // Prevent the default console error output
    event.preventDefault();
  });

  // Handle general JavaScript errors
  window.addEventListener('error', (event) => {
    console.error('Global JavaScript Error:', {
      message: event.message,
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      error: event.error,
      timestamp: new Date().toISOString()
    });
  });
}