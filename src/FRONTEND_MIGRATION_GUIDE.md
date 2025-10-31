# Frontend Migration Guide - New Backend Architecture

## 📋 **Overview**

This guide provides step-by-step instructions for migrating the frontend from the current architecture to the new optimized version that works seamlessly with the improved Rust backend.

## 🏗️ **What Changed**

### **New Architecture Components**
- **API Service Layer** (`src/services/api.js`) - Centralized Tauri command handling
- **Error Handling System** (`src/utils/errorHandler.js`) - Type-safe error management
- **Optimized Auth Context** (`src/contexts/AuthContextOptimized.jsx`) - Enhanced authentication
- **New Hooks** (`src/hooks/useOptimizedApi.js`) - React hooks for new backend patterns

### **Key Improvements**
- ✅ **Type Safety**: Better error handling and validation
- ✅ **Separation of Concerns**: Clear API, error, and business logic layers
- ✅ **Performance**: Optimized loading states and caching
- ✅ **Maintainability**: Cleaner, more modular code structure
- ✅ **Developer Experience**: Better debugging and development tools

---

## 📁 **File Changes Summary**

### **New Files Created**
```
src/
├── services/
│   └── api.js                          # ✅ NEW: Complete API service layer
├── utils/
│   └── errorHandler.js                 # ✅ NEW: Centralized error handling
├── contexts/
│   └── AuthContextOptimized.jsx        # ✅ NEW: Enhanced authentication
└── hooks/
    └── useOptimizedApi.js              # ✅ NEW: Optimized React hooks
```

### **Existing Files to Update**
```
src/
├── contexts/AuthContext.jsx            # 🔄 UPDATE: Use new AuthContextOptimized
├── contexts/auth.js                    # 🔄 UPDATE: Export new useAuth hook
├── main.jsx                            # 🔄 UPDATE: Import new providers
├── components/...                      # 🔄 UPDATE: Use new hooks (gradually)
└── hooks/...                           # 🔄 UPDATE: Replace with optimized versions
```

---

## 🚀 **Migration Steps**

### **Phase 1: Setup (Immediate)**

#### **Step 1: Add New Files**
```bash
# Copy the new files to your project
cp src/services/api.js your-project/src/services/
cp src/utils/errorHandler.js your-project/src/utils/
cp src/contexts/AuthContextOptimized.jsx your-project/src/contexts/
cp src/hooks/useOptimizedApi.js your-project/src/hooks/
```

#### **Step 2: Update Main Entry Point**
Replace `src/main.jsx`:

**Before:**
```jsx
import { AuthProvider } from '@/contexts/AuthContext'
import { NotificationProvider } from '@/contexts/NotificationContext'
import { ThemeProvider } from '@/contexts/ThemeContext'

<AuthProvider>
  <NotificationProvider>
    <ThemeProvider>
      <RouterProvider router={router} />
    </ThemeProvider>
  </NotificationProvider>
</AuthProvider>
```

**After:**
```jsx
import { AuthProvider } from '@/contexts/AuthContextOptimized'
import { NotificationProvider } from '@/contexts/NotificationContext'
import { ThemeProvider } from '@/contexts/ThemeContext'

<AuthProvider>
  <NotificationProvider>
    <ThemeProvider>
      <RouterProvider router={router} />
    </ThemeProvider>
  </NotificationProvider>
</AuthProvider>
```

#### **Step 3: Update Auth Context Export**
Replace `src/contexts/auth.js`:

**Before:**
```jsx
export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
};
```

**After:**
```jsx
// Re-export the new optimized context
export { AuthContext } from './AuthContextOptimized';
export { useAuth as useAuthLegacy } from './AuthContextOptimized';

// Keep old export for backward compatibility during migration
export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return {
    // Map new context to old interface for compatibility
    user: context.user,
    token: context.token,
    login: context.login,
    logout: context.logout,
    loading: context.loading,
    // New features
    isAuthenticated: context.isAuthenticated,
    isAdmin: context.isAdmin,
    displayName: context.displayName,
    roleDisplay: context.roleDisplay,
    updatePassword: context.updatePassword,
    validateToken: context.validateToken
  };
};
```

### **Phase 2: Gradual Component Migration**

#### **Step 4: Update Authentication Components**

**Update `src/components/Authentication/Login.jsx`:**

**Before:**
```jsx
const handleSubmit = async (event) => {
  event.preventDefault();
  try {
    const response = await invoke('login', { 
      request: { username, password } 
    });
    login(response.user, response.token);
    showNotification(`Welcome, ${response.user.username}!`, 'success');
  } catch (error) {
    showNotification(error.message || 'Login failed', 'error');
  }
};
```

**After:**
```jsx
import { useAuth } from '@/contexts/auth';
import { authService } from '@/services/api';

const handleSubmit = async (event) => {
  event.preventDefault();
  
  try {
    const result = await authService.login({ username, password });
    
    if (result.success) {
      login(result.data.user, result.data.token);
      showNotification(`Welcome, ${result.data.user.username}!`, 'success');
    } else {
      showNotification(result.error.message || 'Login failed', 'error');
    }
  } catch (error) {
    showNotification('Login failed. Please try again.', 'error');
  }
};
```

#### **Step 5: Update Client Management Components**

**Update `src/components/ClientManagement/index.jsx`:**

**Before:**
```jsx
const { clients, fetchData } = useAppStore();

const handleAddClient = async () => {
  try {
    const response = await invoke('add_client', {
      token,
      request: { name, mac, ip, master, snapshot }
    });
    showNotification(response.message, 'success');
    fetchData();
  } catch (error) {
    showNotification(error, 'error');
  }
};
```

**After:**
```jsx
import { useClients } from '@/hooks/useOptimizedApi';

const { 
  clients, 
  isLoading, 
  error, 
  actions: { addClient, deleteClient, controlClient } 
} = useClients({ autoRefresh: true });

const handleAddClient = async () => {
  const result = await addClient({ name, mac, ip, master, snapshot });
  
  if (result.success) {
    showNotification(result.data.message, 'success');
  } else {
    showNotification(result.message, 'error');
  }
};
```

#### **Step 6: Update Service Management Hooks**

**Replace `src/hooks/useServiceManager.js`:**

**Before:**
```jsx
export const useServiceManager = () => {
  const { showNotification } = useNotification();
  
  const handleServiceAction = useCallback(async (serviceKey, action) => {
    const token = localStorage.getItem('authToken') || '';
    await invoke('control_service', {
      token,
      serviceKey: serviceKey,
      req: { action: action }
    }).then((response) => {
      if (response.message) showNotification(response.message, 'success');
    }).catch((error) => showNotification(error, 'error',));
  }, [showNotification]);
};
```

**After:**
```jsx
import { useServices } from '@/hooks/useOptimizedApi';

export const useServiceManager = () => {
  const { isLoading, error, actions } = useServices();
  
  const handleServiceAction = async (serviceKey, action) => {
    const result = await actions.controlService(serviceKey, action);
    
    if (result.success) {
      showNotification(result.data.message, 'success');
    } else {
      showNotification(result.message, 'error');
    }
  };
  
  return {
    handleServiceAction,
    handleServiceConfigView: actions.getServiceConfig,
    handleConfigSave: actions.saveServiceConfig,
    isLoading,
    error
  };
};
```

### **Phase 3: Advanced Features Migration**

#### **Step 7: Implement Error Boundaries**

**Create `src/components/ErrorBoundaryOptimized.jsx`:**

```jsx
import { ErrorBoundary } from '@/utils/errorHandler';
import { useErrorHandler } from '@/utils/errorHandler';

const ErrorBoundaryOptimized = ({ children, fallback = null }) => {
  return (
    <ErrorBoundary>
      {children}
    </ErrorBoundary>
  );
};

export default ErrorBoundaryOptimized;
```

**Update `src/components/ErrorBoundary.jsx`:**

```jsx
import { ErrorBoundary as BaseErrorBoundary } from '@/utils/errorHandler';

class ErrorBoundary extends BaseErrorBoundary {
  render() {
    if (this.state.hasError) {
      return (
        <div className="error-boundary">
          <h2>Something went wrong.</h2>
          <details>
            <summary>Error Details</summary>
            <pre>{this.state.error?.toString()}</pre>
          </details>
          <button onClick={() => window.location.reload()}>
            Reload Page
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
```

#### **Step 8: Add Type Safety**

**Create `src/types/index.js`:**

```javascript
/**
 * @typedef {Object} Client
 * @property {string} id - Client identifier
 * @property {string} name - Client name
 * @property {string} mac - MAC address
 * @property {string} ip - IP address
 * @property {string} master - Master image name
 * @property {string|null} snapshot - Snapshot name
 * @property {string|null} status - Client status
 */

/**
 * @typedef {Object} User
 * @property {string} id - User ID
 * @property {string} username - Username
 * @property {string} role - User role
 */

/**
 * @typedef {Object} ApiResponse
 * @property {boolean} success - Operation success status
 * @property {any} data - Response data
 * @property {string} message - Response message
 * @property {Object} error - Error details if failed
 */

// Export types for use in components
export {};
```

---

## 🧪 **Testing Strategy**

### **Component Testing**

**Test old vs new functionality:**
```javascript
// Test new hook
import { renderHook } from '@testing-library/react';
import { useClients } from '@/hooks/useOptimizedApi';

test('useClients hook provides clients data', () => {
  const { result } = renderHook(() => useClients());
  
  expect(result.current.clients).toBeDefined();
  expect(result.current.actions).toBeDefined();
  expect(typeof result.current.actions.addClient).toBe('function');
});
```

### **Integration Testing**

**Test API service integration:**
```javascript
// Test API service
import { clientService } from '@/services/api';

test('clientService addClient validates input', async () => {
  const invalidData = { name: '', mac: 'invalid', ip: 'invalid' };
  
  await expect(clientService.addClient(invalidData)).rejects.toThrow();
});
```

### **End-to-End Testing**

**Test complete user flows:**
1. Login with new authentication system
2. Create client using new hooks
3. Verify error handling works correctly
4. Test service management operations

---

## 🔄 **Rollback Procedure**

If issues arise during migration:

### **Immediate Rollback**
1. **Restore Original Files:**
   ```bash
   git checkout HEAD~1 -- src/
   ```

2. **Clear New Dependencies:**
   ```bash
   rm src/services/api.js
   rm src/utils/errorHandler.js
   rm src/contexts/AuthContextOptimized.jsx
   rm src/hooks/useOptimizedApi.js
   ```

3. **Restart Application:**
   ```bash
   npm run dev
   ```

### **Partial Rollback**
- Keep new files but revert specific components to old patterns
- Use feature flags to disable new functionality
- Maintain backward compatibility interfaces

---

## 📊 **Performance Considerations**

### **Memory Usage**
- New hooks cache API responses automatically
- Error handler prevents memory leaks
- Optimized authentication reduces token storage

### **Loading Performance**
- Parallel API calls where possible
- Intelligent caching strategies
- Optimistic UI updates

### **Bundle Size**
- Tree-shaking friendly modular design
- Minimal runtime overhead
- Type safety without performance cost

---

## 🎯 **Best Practices Going Forward**

### **Code Organization**
1. **Keep API calls in service layer only**
2. **Use hooks for all business logic**
3. **Handle errors at the appropriate level**
4. **Validate input before API calls**

### **Error Handling**
1. **Use AppError types consistently**
2. **Provide user-friendly error messages**
3. **Log errors for debugging**
4. **Handle authentication errors automatically**

### **Component Design**
1. **Keep components focused on presentation**
2. **Use custom hooks for data fetching**
3. **Leverage memoization for performance**
4. **Implement proper loading states**

### **Testing**
1. **Test hooks in isolation**
2. **Mock API services for unit tests**
3. **Test error scenarios thoroughly**
4. **Verify user interactions work correctly**

---

## 🆘 **Troubleshooting**

### **Common Issues**

**1. Authentication Failures**
```javascript
// Check token validation
const { isAuthenticated, validateToken } = useAuth();
await validateToken(); // Force validation
```

**2. API Call Failures**
```javascript
// Check error details
const { error, actions } = useClients();
console.log(error); // See detailed error info
```

**3. Hook Performance Issues**
```javascript
// Check dependencies
useClients({ autoRefresh: true, refreshInterval: 30000 });
```

### **Debug Tools**

**Enable debug logging:**
```javascript
// In development environment
localStorage.setItem('debug', 'diskless:*');
```

**Check hook state:**
```javascript
const { _debug } = useAuth(); // Access debug info
console.log(_debug);
```

---

## 📞 **Support**

For questions or issues during migration:

1. **Check the troubleshooting section above**
2. **Review the new API documentation**
3. **Test individual components in isolation**
4. **Use the rollback procedure if needed**

---

## 🎉 **Benefits After Migration**

Once migration is complete, you'll have:

- ✅ **Better Error Handling**: Type-safe errors with user-friendly messages
- ✅ **Improved Performance**: Optimized API calls and caching
- ✅ **Easier Maintenance**: Clear separation of concerns
- ✅ **Enhanced Developer Experience**: Better debugging tools
- ✅ **Future-Proof Architecture**: Easy to extend and modify
- ✅ **Better Type Safety**: Fewer runtime errors
- ✅ **Consistent Patterns**: Standardized throughout the application

The migration effort will pay off immediately with improved code quality and developer productivity, while providing a solid foundation for future enhancements.