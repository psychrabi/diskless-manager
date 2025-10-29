//! Optimized Hooks for New Backend Architecture
//! 
//! These hooks follow the patterns established in the backend with proper
//! error handling, loading states, and dependency injection.

import { useState, useEffect, useCallback, useMemo } from 'react';
import { 
  clientService, 
  serviceService, 
  imageService, 
  diskService,
  systemService,
  configService,
  licenseService 
} from '@/services/api';
import { useErrorHandler } from '@/utils/errorHandler';
import { useAuth } from '@/contexts/auth';
import { useNotification } from '@/contexts/notification';

// ========== HOOK BASE CLASS ==========
/**
 * Base class for API hooks providing common functionality
 */
class ApiHookBase {
  /**
   * Create a loading state manager
   */
  static createLoadingState(initialState = {}) {
    return {
      isLoading: false,
      error: null,
      data: null,
      ...initialState
    };
  }

  /**
   * Create async operation handler
   */
  static createAsyncHandler(errorHandler, showNotification) {
    return async (operation, options = {}) => {
      const { silent = false, context = 'API Operation' } = options;
      
      try {
        const result = await operation();
        return { success: true, data: result };
      } catch (error) {
        const appError = errorHandler.handleError(error, context, { silent });
        
        return {
          success: false,
          error: appError,
          message: errorHandler.getUserFriendlyMessage(appError)
        };
      }
    };
  }

  /**
   * Create re-fetch function
   */
  static createRefetchFunction(loadingState, setLoadingState, asyncHandler) {
    return useCallback(async (operation, options = {}) => {
      setLoadingState(prev => ({ ...prev, isLoading: true, error: null }));
      
      const result = await asyncHandler(operation, options);
      
      setLoadingState(prev => ({
        ...prev,
        isLoading: false,
        data: result.success ? result.data : prev.data,
        error: result.success ? null : result.error
      }));
      
      return result;
    }, [asyncHandler, setLoadingState]);
  }
}

// ========== CLIENT MANAGEMENT HOOK ==========
/**
 * Optimized client management hook
 */
export function useClients(options = {}) {
  const { autoRefresh = false, refreshInterval = 30000 } = options;
  const [state, setState] = useState(() => ApiHookBase.createLoadingState({
    clients: [],
    clientOverview: null
  }));
  
  const errorHandler = useErrorHandler();
  const { token } = useAuth();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Fetch clients data
   */
  const fetchClients = useCallback(async () => {
    const result = await refetch(
      () => clientService.getClients(),
      { context: 'Fetch clients' }
    );
    
    if (result.success && result.data) {
      setState(prev => ({
        ...prev,
        clients: Array.isArray(result.data) ? result.data : [],
        data: result.data
      }));
    }
    
    return result;
  }, [refetch]);

  /**
   * Add new client
   */
  const addClient = useCallback(async (clientData) => {
    const result = await refetch(
      () => clientService.addClient(clientData),
      { context: 'Add client', silent: false }
    );
    
    if (result.success) {
      // Refresh clients list after successful addition
      await fetchClients();
    }
    
    return result;
  }, [refetch, fetchClients]);

  /**
   * Edit client
   */
  const editClient = useCallback(async (clientId, updates) => {
    const result = await refetch(
      () => clientService.editClient(clientId, updates),
      { context: 'Edit client', silent: false }
    );
    
    if (result.success) {
      await fetchClients();
    }
    
    return result;
  }, [refetch, fetchClients]);

  /**
   * Delete client
   */
  const deleteClient = useCallback(async (clientId) => {
    const result = await refetch(
      () => clientService.deleteClient(clientId),
      { context: 'Delete client', silent: false }
    );
    
    if (result.success) {
      await fetchClients();
    }
    
    return result;
  }, [refetch, fetchClients]);

  /**
   * Control client
   */
  const controlClient = useCallback(async (clientId, action, options = {}) => {
    const { make_super = null } = options;
    
    const result = await refetch(
      () => clientService.controlClient(clientId, { action, make_super }),
      { context: `Control client: ${action}`, silent: false }
    );
    
    return result;
  }, [refetch]);

  /**
   * Reset client
   */
  const resetClient = useCallback(async (clientId) => {
    const result = await refetch(
      () => clientService.resetClient(clientId),
      { context: 'Reset client', silent: false }
    );
    
    if (result.success) {
      await fetchClients();
    }
    
    return result;
  }, [refetch, fetchClients]);

  /**
   * Remote client connection
   */
  const remoteClient = useCallback(async (clientId) => {
    return refetch(
      () => clientService.remoteClient(clientId),
      { context: 'Remote client', silent: true }
    );
  }, [refetch]);

  /**
   * Get client overview
   */
  const fetchOverview = useCallback(async () => {
    const result = await refetch(
      () => clientService.getClientOverview(),
      { context: 'Fetch client overview', silent: true }
    );
    
    if (result.success && result.data) {
      setState(prev => ({
        ...prev,
        clientOverview: result.data
      }));
    }
    
    return result;
  }, [refetch]);

  // Auto-refresh effect
  useEffect(() => {
    if (autoRefresh && token) {
      fetchClients();
      fetchOverview();
      
      const interval = setInterval(() => {
        if (!state.isLoading) {
          fetchClients();
        }
      }, refreshInterval);
      
      return () => clearInterval(interval);
    }
  }, [autoRefresh, token, refreshInterval, fetchClients, fetchOverview, state.isLoading]);

  // Computed values
  const computedValues = useMemo(() => ({
    clients: state.clients || [],
    clientOverview: state.clientOverview,
    isLoading: state.isLoading,
    error: state.error,
    hasError: !!state.error,
    isEmpty: state.clients && state.clients.length === 0,
    totalClients: state.clients?.length || 0,
    onlineClients: state.clients?.filter(client => client.status === 'Online').length || 0
  }), [state]);

  return {
    ...computedValues,
    actions: {
      fetchClients,
      addClient,
      editClient,
      deleteClient,
      controlClient,
      resetClient,
      remoteClient,
      fetchOverview,
      refetch: fetchClients
    }
  };
}

// ========== SERVICE MANAGEMENT HOOK ==========
/**
 * Optimized service management hook
 */
export function useServices(options = {}) {
  const { zfsPool = 'diskless' } = options;
  const [state, setState] = useState(() => ApiHookBase.createLoadingState());
  const errorHandler = useErrorHandler();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Fetch services status
   */
  const fetchServices = useCallback(async () => {
    return refetch(
      () => serviceService.getServices(zfsPool),
      { context: 'Fetch services' }
    );
  }, [refetch, zfsPool]);

  /**
   * Control service
   */
  const controlService = useCallback(async (serviceKey, action) => {
    const result = await refetch(
      () => serviceService.controlService(serviceKey, action),
      { context: `Control service: ${serviceKey}`, silent: false }
    );
    
    if (result.success) {
      // Refresh services list after control action
      setTimeout(fetchServices, 1000); // Give service time to start/stop
    }
    
    return result;
  }, [refetch, fetchServices]);

  /**
   * Get service config
   */
  const getServiceConfig = useCallback(async (serviceKey) => {
    return refetch(
      () => serviceService.getServiceConfig(serviceKey),
      { context: `Get service config: ${serviceKey}`, silent: true }
    );
  }, [refetch]);

  /**
   * Save service config
   */
  const saveServiceConfig = useCallback(async (serviceKey, content) => {
    const result = await refetch(
      () => serviceService.saveServiceConfig(serviceKey, content),
      { context: `Save service config: ${serviceKey}`, silent: false }
    );
    
    if (result.success) {
      await fetchServices();
    }
    
    return result;
  }, [refetch, fetchServices]);

  /**
   * Check package status
   */
  const checkPackageStatus = useCallback(async () => {
    return refetch(
      () => serviceService.checkPackageStatus(),
      { context: 'Check package status', silent: true }
    );
  }, [refetch]);

  /**
   * Install packages
   */
  const installPackages = useCallback(async () => {
    return refetch(
      () => serviceService.installPackages(),
      { context: 'Install packages', silent: false }
    );
  }, [refetch]);

  return {
    data: state.data,
    isLoading: state.isLoading,
    error: state.error,
    actions: {
      fetchServices,
      controlService,
      getServiceConfig,
      saveServiceConfig,
      checkPackageStatus,
      installPackages
    }
  };
}

// ========== IMAGE MANAGEMENT HOOK ==========
/**
 * Optimized image management hook
 */
export function useImages(options = {}) {
  const [state, setState] = useState(() => ApiHookBase.createLoadingState());
  const errorHandler = useErrorHandler();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Fetch images
   */
  const fetchImages = useCallback(async () => {
    return refetch(
      () => imageService.getImages(),
      { context: 'Fetch images' }
    );
  }, [refetch]);

  /**
   * Create image
   */
  const createImage = useCallback(async (name, size) => {
    const result = await refetch(
      () => imageService.createImage(name, size),
      { context: 'Create image', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  /**
   * Delete image
   */
  const deleteImage = useCallback(async (imageName) => {
    const result = await refetch(
      () => imageService.deleteImage(imageName),
      { context: 'Delete image', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  /**
   * Create snapshot
   */
  const createSnapshot = useCallback(async (masterName, snapshotName = null) => {
    const result = await refetch(
      () => imageService.createSnapshot(masterName, snapshotName),
      { context: 'Create snapshot', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  /**
   * Delete snapshot
   */
  const deleteSnapshot = useCallback(async (masterName, snapshotName) => {
    const result = await refetch(
      () => imageService.deleteSnapshot(masterName, snapshotName),
      { context: 'Delete snapshot', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  /**
   * Rollback snapshot
   */
  const rollbackSnapshot = useCallback(async (masterName, snapshotName) => {
    const result = await refetch(
      () => imageService.rollbackSnapshot(masterName, snapshotName),
      { context: 'Rollback snapshot', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  /**
   * Set default image
   */
  const setDefaultImage = useCallback(async (name) => {
    const result = await refetch(
      () => imageService.setDefaultImage(name),
      { context: 'Set default image', silent: false }
    );
    
    if (result.success) {
      await fetchImages();
    }
    
    return result;
  }, [refetch, fetchImages]);

  return {
    data: state.data,
    images: Array.isArray(state.data) ? state.data : [],
    isLoading: state.isLoading,
    error: state.error,
    actions: {
      fetchImages,
      createImage,
      deleteImage,
      createSnapshot,
      deleteSnapshot,
      rollbackSnapshot,
      setDefaultImage
    }
  };
}

// ========== SYSTEM HOOK ==========
/**
 * Optimized system information hook
 */
export function useSystem() {
  const [state, setState] = useState(() => ApiHookBase.createLoadingState({
    serverInfo: null,
    ramUsage: null,
    disks: []
  }));
  const errorHandler = useErrorHandler();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Fetch server info
   */
  const fetchServerInfo = useCallback(async () => {
    const result = await refetch(
      () => systemService.getServerInfo(),
      { context: 'Fetch server info', silent: true }
    );
    
    if (result.success && result.data) {
      setState(prev => ({
        ...prev,
        serverInfo: result.data
      }));
    }
    
    return result;
  }, [refetch]);

  /**
   * Fetch RAM usage
   */
  const fetchRamUsage = useCallback(async () => {
    const result = await refetch(
      () => systemService.getRamUsage(),
      { context: 'Fetch RAM usage', silent: true }
    );
    
    if (result.success && result.data) {
      setState(prev => ({
        ...prev,
        ramUsage: result.data
      }));
    }
    
    return result;
  }, [refetch]);

  /**
   * Clear RAM cache
   */
  const clearRamCache = useCallback(async () => {
    return refetch(
      () => systemService.clearRamCache(),
      { context: 'Clear RAM cache', silent: false }
    );
  }, [refetch]);

  /**
   * List disks
   */
  const listDisks = useCallback(async () => {
    const result = await refetch(
      () => systemService.listDisks(),
      { context: 'List disks', silent: true }
    );
    
    if (result.success && result.data) {
      setState(prev => ({
        ...prev,
        disks: Array.isArray(result.data) ? result.data : []
      }));
    }
    
    return result;
  }, [refetch]);

  /**
   * Get service logs
   */
  const getServiceLogs = useCallback(async (unit, lines = 200) => {
    return refetch(
      () => systemService.getServiceLogs(unit, lines),
      { context: `Get service logs: ${unit}`, silent: true }
    );
  }, [refetch]);

  /**
   * Get application logs
   */
  const getLogs = useCallback(async () => {
    return refetch(
      () => systemService.getLogs(),
      { context: 'Get application logs', silent: true }
    );
  }, [refetch]);

  /**
   * Clear application logs
   */
  const clearLogs = useCallback(async () => {
    return refetch(
      () => systemService.clearLogs(),
      { context: 'Clear application logs', silent: false }
    );
  }, [refetch]);

  // Fetch server info on mount
  useEffect(() => {
    fetchServerInfo();
  }, [fetchServerInfo]);

  // Computed values
  const computedValues = useMemo(() => ({
    serverInfo: state.serverInfo,
    ramUsage: state.ramUsage,
    disks: state.disks || [],
    isLoading: state.isLoading,
    error: state.error
  }), [state]);

  return {
    ...computedValues,
    actions: {
      fetchServerInfo,
      fetchRamUsage,
      clearRamCache,
      listDisks,
      getServiceLogs,
      getLogs,
      clearLogs,
      refetch: fetchServerInfo
    }
  };
}

// ========== CONFIGURATION HOOK ==========
/**
 * Optimized configuration management hook
 */
export function useConfig() {
  const [state, setState] = useState(() => ApiHookBase.createLoadingState());
  const errorHandler = useErrorHandler();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Read configuration
   */
  const readConfig = useCallback(async () => {
    return refetch(
      () => configService.readConfig(),
      { context: 'Read configuration', silent: true }
    );
  }, [refetch]);

  /**
   * Save configuration
   */
  const saveConfig = useCallback(async (config) => {
    const result = await refetch(
      () => configService.saveConfig(config),
      { context: 'Save configuration', silent: false }
    );
    
    return result;
  }, [refetch]);

  return {
    data: state.data,
    isLoading: state.isLoading,
    error: state.error,
    actions: {
      readConfig,
      saveConfig,
      refetch: readConfig
    }
  };
}

// ========== LICENSE HOOK ==========
/**
 * Optimized license management hook
 */
export function useLicense() {
  const [state, setState] = useState(() => ApiHookBase.createLoadingState());
  const errorHandler = useErrorHandler();
  const asyncHandler = ApiHookBase.createAsyncHandler(errorHandler, useNotification());
  const refetch = ApiHookBase.createRefetchFunction(state, setState, asyncHandler);

  /**
   * Get license info
   */
  const getLicenseInfo = useCallback(async () => {
    return refetch(
      () => licenseService.getLicenseInfo(),
      { context: 'Get license info', silent: true }
    );
  }, [refetch]);

  /**
   * Activate license
   */
  const activateLicense = useCallback(async (licenseKey) => {
    const result = await refetch(
      () => licenseService.activateLicense(licenseKey),
      { context: 'Activate license', silent: false }
    );
    
    if (result.success) {
      await getLicenseInfo();
    }
    
    return result;
  }, [refetch, getLicenseInfo]);

  return {
    data: state.data,
    isLoading: state.isLoading,
    error: state.error,
    actions: {
      getLicenseInfo,
      activateLicense,
      refetch: getLicenseInfo
    }
  };
}