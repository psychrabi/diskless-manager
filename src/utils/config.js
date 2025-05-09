// API Configuration
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://192.168.1.250:5000/api';

// Application Configuration
export const APP_CONFIG = {
  refreshInterval: 30000, // 30 seconds
  maxRetries: 3,
  retryDelay: 1000, // 1 second
};

// Theme Configuration
export const THEME_CONFIG = {
  light: {
    primary: '#3B82F6',
    secondary: '#6B7280',
    background: '#F3F4F6',
    text: '#1F2937',
  },
  dark: {
    primary: '#60A5FA',
    secondary: '#9CA3AF',
    background: '#111827',
    text: '#F9FAFB',
  },
}; 