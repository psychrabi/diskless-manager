//! Optimized Authentication Context
//! 
//! Updated to work with the new API service layer and follow
//! the architectural patterns established in the backend.

import { useState, useEffect, useCallback, useMemo } from 'react';
import { authService } from '@/services/api';
import { AuthContext } from './auth';
import { useNotification } from '@/contexts/notification';
import { AppError, ErrorTypes, createError } from '@/utils/errorHandler';

export const AuthProvider = ({ children }) => {
  // ========== STATE ==========
  const [user, setUser] = useState(null);
  const [token, setToken] = useState(null);
  const [loading, setLoading] = useState(true);
  const [isValidating, setIsValidating] = useState(false);
  const { showNotification } = useNotification();

  // ========== TOKEN MANAGEMENT ==========
  /**
   * Store authentication token in localStorage
   */
  const storeToken = useCallback((authToken, userData) => {
    try {
      localStorage.setItem('authToken', authToken);
      localStorage.setItem('user', JSON.stringify(userData));
      setToken(authToken);
      setUser(userData);
      return true;
    } catch (error) {
      console.error('Failed to store authentication data:', error);
      return false;
    }
  }, []);

  /**
   * Clear authentication token from localStorage
   */
  const clearToken = useCallback(() => {
    try {
      localStorage.removeItem('authToken');
      localStorage.removeItem('user');
      setToken(null);
      setUser(null);
    } catch (error) {
      console.error('Failed to clear authentication data:', error);
    }
  }, []);

  /**
   * Get stored authentication data
   */
  const getStoredAuth = useCallback(() => {
    try {
      const storedToken = localStorage.getItem('authToken');
      const storedUserStr = localStorage.getItem('user');
      
      if (storedToken && storedUserStr) {
        const userData = JSON.parse(storedUserStr);
        return { token: storedToken, user: userData };
      }
    } catch (error) {
      console.error('Failed to retrieve stored authentication data:', error);
    }
    return { token: null, user: null };
  }, []);

  // ========== TOKEN VALIDATION ==========
  /**
   * Validate authentication token with backend
   */
  const validateTokenAsync = useCallback(async (authToken) => {
    try {
      const claims = await authService.validateToken(authToken);
      
      // Extract user information from claims
      const userData = {
        id: claims.sub || 'unknown',
        username: claims.username || 'unknown',
        role: claims.role || 'user'
      };
      
      return { valid: true, user: userData };
    } catch (error) {
      return { valid: false, error };
    }
  }, []);

  /**
   * Validate stored token on app startup
   */
  const validateStoredToken = useCallback(async () => {
    const { token: storedToken, user: storedUser } = getStoredAuth();
    
    if (!storedToken || !storedUser) {
      setLoading(false);
      return false;
    }

    setIsValidating(true);
    
    try {
      const validationResult = await validateTokenAsync(storedToken);
      
      if (validationResult.valid) {
        // Update user data if claims differ from stored user
        const updatedUser = validationResult.user;
        if (JSON.stringify(updatedUser) !== JSON.stringify(storedUser)) {
          storeToken(storedToken, updatedUser);
        }
        setToken(storedToken);
        setUser(updatedUser);
        return true;
      } else {
        // Token is invalid, clear it
        clearToken();
        showNotification(
          'Session expired. Please log in again.',
          'warning'
        );
        return false;
      }
    } catch (error) {
      console.error('Token validation error:', error);
      clearToken();
      
      // Show user-friendly error
      const appError = createError(error, 'Token validation');
      if (appError.type !== ErrorTypes.AUTH) {
        showNotification(
          'Failed to validate session. Please log in again.',
          'error'
        );
      }
      
      return false;
    } finally {
      setIsValidating(false);
      setLoading(false);
    }
  }, [getStoredAuth, validateTokenAsync, storeToken, clearToken, showNotification]);

  // ========== AUTHENTICATION ACTIONS ==========
  /**
   * Perform user login
   */
  const login = useCallback(async (loginData) => {
    try {
      setLoading(true);
      
      // Validate input format
      if (!loginData.username?.trim() || !loginData.password?.trim()) {
        throw new AppError('Username and password are required', ErrorTypes.VALIDATION);
      }
      
      // Call authentication service
      const response = await authService.login({
        username: loginData.username.trim(),
        password: loginData.password
      });
      
      // Store authentication data
      const success = storeToken(response.token, response.user);
      if (!success) {
        throw new AppError('Failed to store authentication data', ErrorTypes.SYSTEM);
      }
      
      showNotification(
        `Welcome back, ${response.user.username}!`,
        'success'
      );
      
      return { success: true, user: response.user };
      
    } catch (error) {
      console.error('Login error:', error);
      
      // Create user-friendly error message
      let errorMessage = 'Login failed. Please check your credentials.';
      
      if (error instanceof AppError) {
        switch (error.type) {
          case ErrorTypes.AUTH:
            errorMessage = 'Invalid username or password.';
            break;
          case ErrorTypes.VALIDATION:
            errorMessage = error.message;
            break;
          case ErrorTypes.NETWORK:
            errorMessage = 'Network error. Please check your connection.';
            break;
          default:
            errorMessage = error.message || 'Login failed. Please try again.';
        }
      }
      
      showNotification(errorMessage, 'error');
      
      return { 
        success: false, 
        error: errorMessage,
        details: error instanceof AppError ? error : createError(error, 'Login')
      };
      
    } finally {
      setLoading(false);
    }
  }, [storeToken, showNotification]);

  /**
   * Update admin password
   */
  const updatePassword = useCallback(async (oldPassword, newPassword) => {
    try {
      if (!token) {
        throw new AppError('Authentication required', ErrorTypes.AUTH);
      }
      
      if (!oldPassword?.trim() || !newPassword?.trim()) {
        throw new AppError('Both old and new passwords are required', ErrorTypes.VALIDATION);
      }
      
      if (newPassword.length < 6) {
        throw new AppError('Password must be at least 6 characters long', ErrorTypes.VALIDATION);
      }
      
      await authService.updateAdminPassword(oldPassword, newPassword, token);
      
      showNotification('Password updated successfully', 'success');
      
      return { success: true };
      
    } catch (error) {
      console.error('Password update error:', error);
      
      let errorMessage = 'Failed to update password. Please try again.';
      
      if (error instanceof AppError) {
        switch (error.type) {
          case ErrorTypes.AUTH:
            errorMessage = 'Session expired. Please log in again.';
            break;
          case ErrorTypes.VALIDATION:
            errorMessage = error.message;
            break;
          default:
            errorMessage = error.message;
        }
      }
      
      showNotification(errorMessage, 'error');
      
      return { 
        success: false, 
        error: errorMessage,
        details: error instanceof AppError ? error : createError(error, 'Password update')
      };
    }
  }, [token, showNotification]);

  /**
   * Logout user and clear session
   */
  const logout = useCallback(() => {
    clearToken();
    showNotification('Logged out successfully', 'info');
  }, [clearToken, showNotification]);

  // ========== INITIALIZATION ==========
  useEffect(() => {
    // Validate stored token on component mount
    validateStoredToken();
  }, [validateStoredToken]);

  // ========== COMPUTED VALUES ==========
  /**
   * Check if user is authenticated
   */
  const isAuthenticated = useMemo(() => {
    return !!(token && user && !isValidating);
  }, [token, user, isValidating]);

  /**
   * Check if user has admin role
   */
  const isAdmin = useMemo(() => {
    return user?.role === 'admin';
  }, [user]);

  /**
   * Get user display name
   */
  const displayName = useMemo(() => {
    return user?.username || 'Unknown User';
  }, [user]);

  /**
   * Get user role display text
   */
  const roleDisplay = useMemo(() => {
    return user?.role === 'admin' ? 'Administrator' : 'User';
  }, [user]);

  // ========== CONTEXT VALUE ==========
  const value = useMemo(() => ({
    // State
    user,
    token,
    loading: loading || isValidating,
    isValidating,
    isAuthenticated,
    isAdmin,
    
    // Computed values
    displayName,
    roleDisplay,
    
    // Actions
    login,
    logout,
    updatePassword,
    validateToken: validateTokenAsync,
    
    // Utility functions
    hasRole: (requiredRole) => user?.role === requiredRole,
    getAuthHeader: () => token ? { Authorization: `Bearer ${token}` } : {},
    
    // Debug info
    _debug: {
      userData: user,
      tokenPresent: !!token,
      tokenLength: token?.length || 0,
      tokenPrefix: token ? `${token.substring(0, 10)}...` : 'none'
    }
  }), [
    user, 
    token, 
    loading, 
    isValidating, 
    isAuthenticated, 
    isAdmin,
    displayName, 
    roleDisplay,
    login,
    logout,
    updatePassword,
    validateTokenAsync
  ]);

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
};

// ========== LEGACY COMPATIBILITY ==========
// Keep the old useAuth hook for backward compatibility
export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
};