//! Frontend API Service Layer
//! 
//! This module provides a clean, typed interface to the Rust backend
//! following the same architectural patterns as the backend.

import { invoke } from '@tauri-apps/api/core';
import { createErrorHandler } from '@/utils/errorHandler';

// ========== TYPE DEFINITIONS ==========
/**
 * @typedef {Object} Client
 * @property {string} id - Client identifier
 * @property {string} name - Client name
 * @property {string} mac - MAC address
 * @property {string} ip - IP address
 * @property {string} master - Master image name
 * @property {string|null} snapshot - Snapshot name (optional)
 * @property {string|null} status - Client status (optional)
 * @property {string|null} mode - Client mode (optional)
 */

/**
 * @typedef {Object} AddClientRequest
 * @property {string} name - Client name
 * @property {string} mac - MAC address
 * @property {string} ip - IP address
 * @property {string} master - Master image name
 * @property {string|null} snapshot - Snapshot name (optional)
 */

/**
 * @typedef {Object} ControlRequest
 * @property {string} action - Action to perform (wake, reboot, shutdown, etc.)
 * @property {boolean|null} make_super - Whether to enable super mode
 */

/**
 * @typedef {Object} LoginRequest
 * @property {string} username - Username
 * @property {string} password - Password
 */

/**
 * @typedef {Object} User
 * @property {string} id - User ID
 * @property {string} username - Username
 * @property {string} role - User role
 */

/**
 * @typedef {Object} LoginResponse
 * @property {string} token - JWT token
 * @property {User} user - User information
 */

// ========== BASE API CLASS ==========
/**
 * Base class for API services providing common functionality
 */
class BaseApiService {
  constructor() {
    this.errorHandler = createErrorHandler();
  }

  /**
   * Execute a Tauri command with standardized error handling
   * @param {string} command - Tauri command name
   * @param {Object} args - Command arguments
   * @param {string} context - Operation context for error messages
   * @returns {Promise<any>}
   */
  async executeCommand(command, args = {}, context = 'Unknown operation') {
    try {
      // Add authentication token from localStorage
      const token = localStorage.getItem('authToken');
      if (this.requiresAuth(command) && token) {
        args.token = token;
      }

      const result = await invoke(command, args);
      return result;
    } catch (error) {
      return this.errorHandler.handleApiError(error, context);
    }
  }

  /**
   * Check if a command requires authentication
   * @param {string} command - Command name
   * @returns {boolean}
   */
  requiresAuth(command) {
    // Commands that don't require authentication
    const publicCommands = ['get_server_info', 'read_config'];
    return !publicCommands.includes(command);
  }

  /**
   * Validate required fields in a request object
   * @param {Object} data - Data to validate
   * @param {string[]} requiredFields - Required field names
   * @param {string} context - Validation context
   * @throws {Error} If validation fails
   */
  validateRequiredFields(data, requiredFields, context) {
    for (const field of requiredFields) {
      if (!data[field] || data[field].trim() === '') {
        throw new Error(`${context}: ${field} is required`);
      }
    }
  }

  /**
   * Format error response for consistent UI handling
   * @param {Error|string} error - Error object or message
   * @param {string} context - Operation context
   * @returns {Object}
   */
  formatErrorResponse(error, context) {
    const message = typeof error === 'string' ? error : error.message || 'Unknown error';
    return {
      success: false,
      error: message,
      context,
      timestamp: new Date().toISOString()
    };
  }
}

// ========== AUTHENTICATION SERVICE ==========
/**
 * Authentication and authorization API service
 */
export class AuthService extends BaseApiService {
  /**
   * Perform user login
   * @param {LoginRequest} loginData - Login credentials
   * @returns {Promise<LoginResponse>}
   */
  async login(loginData) {
    this.validateRequiredFields(loginData, ['username', 'password'], 'Login');
    return this.executeCommand('login', { request: loginData }, 'User login');
  }

  /**
   * Validate authentication token
   * @param {string} token - JWT token to validate
   * @returns {Promise<Object>} Claims object
   */
  async validateToken(token) {
    if (!token) {
      throw new Error('Token is required for validation');
    }

    return this.executeCommand('validate_auth_token', { token }, 'Token validation');
  }

  /**
   * Update admin password
   * @param {string} oldPassword - Current password
   * @param {string} newPassword - New password
   * @param {string} token - Current session token
   * @returns {Promise<Object>}
   */
  async updateAdminPassword(oldPassword, newPassword, token) {
    if (!oldPassword || !newPassword || !token) {
      throw new Error('All password fields and token are required');
    }

    return this.executeCommand(
      'update_admin_password',
      { token, oldPassword, newPassword },
      'Password update'
    );
  }
}

// ========== CLIENT MANAGEMENT SERVICE ==========
/**
 * Client management API service
 */
export class ClientService extends BaseApiService {
  /**
   * Get all clients or a specific client
   * @param {string|null} clientId - Optional specific client ID
   * @returns {Promise<Client[]>}
   */
  async getClients(clientId = null) {
    const args = { clientId };
    return this.executeCommand('get_clients', args, 'Get clients');
  }

  /**
   * Get client by ID
   * @param {string} clientId - Client identifier
   * @returns {Promise<Client>}
   */
  async getClient(clientId) {
    if (!clientId) {
      throw new Error('Client ID is required');
    }

    return this.executeCommand('get_clients', { clientId }, 'Get client');
  }

  /**
   * Add a new client
   * @param {AddClientRequest} clientData - Client information
   * @returns {Promise<Object>}
   */
  async addClient(clientData) {
    this.validateRequiredFields(
      clientData,
      ['name', 'mac', 'ip', 'master'],
      'Add client'
    );

    // Validate MAC address format
    const macRegex = /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/;
    if (!macRegex.test(clientData.mac)) {
      throw new Error('Invalid MAC address format');
    }

    // Validate IP address format
    const ipRegex = /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/;
    if (!ipRegex.test(clientData.ip)) {
      throw new Error('Invalid IP address format');
    }

    return this.executeCommand('add_client', { request: clientData }, 'Add client');
  }

  /**
   * Edit client information
   * @param {string} clientId - Client identifier
   * @param {Object} updates - Fields to update
   * @returns {Promise<Object>}
   */
  async editClient(clientId, updates) {
    if (!clientId) {
      throw new Error('Client ID is required');
    }

    return this.executeCommand(
      'edit_client',
      { clientId, data: updates },
      'Edit client'
    );
  }

  /**
   * Delete a client
   * @param {string} clientId - Client identifier
   * @returns {Promise<Object>}
   */
  async deleteClient(clientId) {
    if (!clientId) {
      throw new Error('Client ID is required');
    }

    return this.executeCommand('delete_client', { clientId }, 'Delete client');
  }

  /**
   * Control client (wake, reboot, shutdown, etc.)
   * @param {string} clientId - Client identifier
   * @param {ControlRequest} controlData - Control action
   * @returns {Promise<Object>}
   */
  async controlClient(clientId, controlData) {
    if (!clientId || !controlData.action) {
      throw new Error('Client ID and action are required');
    }

    return this.executeCommand(
      'control_client',
      { clientId, req: controlData },
      'Control client'
    );
  }

  /**
   * Reset client to original state
   * @param {string} clientId - Client identifier
   * @returns {Promise<Object>}
   */
  async resetClient(clientId) {
    if (!clientId) {
      throw new Error('Client ID is required');
    }

    return this.executeCommand('reset_client', { clientId }, 'Reset client');
  }

  /**
   * Get client overview statistics
   * @returns {Promise<Object>} Overview statistics
   */
  async getClientOverview() {
    return this.executeCommand('get_client_overview', {}, 'Get client overview');
  }

  /**
   * Remote desktop connection
   * @param {string} clientId - Client identifier
   * @returns {Promise<Object>}
   */
  async remoteClient(clientId) {
    if (!clientId) {
      throw new Error('Client ID is required');
    }

    return this.executeCommand('remote_client', { clientId }, 'Remote client');
  }
}

// ========== SERVICE MANAGEMENT SERVICE ==========
/**
 * System service management API service
 */
export class ServiceService extends BaseApiService {
  /**
   * Get status of all services
   * @param {string} zfsPool - ZFS pool name
   * @returns {Promise<Object>}
   */
  async getServices(zfsPool) {
    if (!zfsPool) {
      throw new Error('ZFS pool name is required');
    }

    return this.executeCommand('get_services', { zfsPool }, 'Get services');
  }

  /**
   * Control a service (start, stop, restart)
   * @param {string} serviceKey - Service identifier
   * @param {string} action - Action to perform
   * @returns {Promise<Object>}
   */
  async controlService(serviceKey, action) {
    if (!serviceKey || !action) {
      throw new Error('Service key and action are required');
    }

    return this.executeCommand(
      'control_service',
      { serviceKey, req: { action } },
      'Control service'
    );
  }

  /**
   * Get service configuration
   * @param {string} serviceKey - Service identifier
   * @returns {Promise<Object>}
   */
  async getServiceConfig(serviceKey) {
    if (!serviceKey) {
      throw new Error('Service key is required');
    }

    return this.executeCommand('get_service_config', { serviceKey }, 'Get service config');
  }

  /**
   * Save service configuration
   * @param {string} serviceKey - Service identifier
   * @param {string} content - Configuration content
   * @returns {Promise<Object>}
   */
  async saveServiceConfig(serviceKey, content) {
    if (!serviceKey || !content) {
      throw new Error('Service key and content are required');
    }

    return this.executeCommand(
      'save_service_config',
      { serviceKey, content },
      'Save service config'
    );
  }

  /**
   * Check package status
   * @returns {Promise<Object[]>}
   */
  async checkPackageStatus() {
    return this.executeCommand('check_package_status', {}, 'Check package status');
  }

  /**
   * Install required packages
   * @returns {Promise<Object>}
   */
  async installPackages() {
    return this.executeCommand('install_packages', {}, 'Install packages');
  }
}

// ========== IMAGE MANAGEMENT SERVICE ==========
/**
 * Image and snapshot management API service
 */
export class ImageService extends BaseApiService {
  /**
   * Get all images
   * @returns {Promise<Object[]>}
   */
  async getImages() {
    return this.executeCommand('get_images', {}, 'Get images');
  }

  /**
   * Create a new image
   * @param {string} name - Image name
   * @param {string} size - Image size (e.g., "50G")
   * @returns {Promise<Object>}
   */
  async createImage(name, size) {
    if (!name || !size) {
      throw new Error('Image name and size are required');
    }

    return this.executeCommand('create_image', { name, size }, 'Create image');
  }

  /**
   * Delete an image
   * @param {string} imageName - Image name
   * @returns {Promise<Object>}
   */
  async deleteImage(imageName) {
    if (!imageName) {
      throw new Error('Image name is required');
    }

    return this.executeCommand('delete_image', { imageName }, 'Delete image');
  }

  /**
   * Create a snapshot
   * @param {string} masterName - Master image name
   * @param {string} snapshotName - Snapshot name (optional)
   * @returns {Promise<Object>}
   */
  async createSnapshot(masterName, snapshotName = null) {
    if (!masterName) {
      throw new Error('Master image name is required');
    }

    return this.executeCommand(
      'create_snapshot',
      { masterName, snapshotName },
      'Create snapshot'
    );
  }

  /**
   * Delete a snapshot
   * @param {string} masterName - Master image name
   * @param {string} snapshotName - Snapshot name
   * @returns {Promise<Object>}
   */
  async deleteSnapshot(masterName, snapshotName) {
    if (!masterName || !snapshotName) {
      throw new Error('Master name and snapshot name are required');
    }

    return this.executeCommand(
      'delete_snapshot',
      { masterName, snapshotName },
      'Delete snapshot'
    );
  }

  /**
   * Rollback to a snapshot
   * @param {string} masterName - Master image name
   * @param {string} snapshotName - Snapshot name
   * @returns {Promise<Object>}
   */
  async rollbackSnapshot(masterName, snapshotName) {
    if (!masterName || !snapshotName) {
      throw new Error('Master name and snapshot name are required');
    }

    return this.executeCommand(
      'rollback_image_snapshot',
      { masterName, snapshotName },
      'Rollback snapshot'
    );
  }

  /**
   * Set default image
   * @param {string} name - Image name
   * @returns {Promise<Object>}
   */
  async setDefaultImage(name) {
    if (!name) {
      throw new Error('Image name is required');
    }

    return this.executeCommand('set_default_image', { name }, 'Set default image');
  }
}

// ========== DISK MANAGEMENT SERVICE ==========
/**
 * Disk and storage management API service
 */
export class DiskService extends BaseApiService {
  /**
   * List ZFS pools
   * @returns {Promise<Object[]>}
   */
  async listZpools() {
    return this.executeCommand('list_zpools', {}, 'List ZFS pools');
  }

  /**
   * List datasets in a pool
   * @param {string} zpool - ZFS pool name
   * @returns {Promise<Object[]>}
   */
  async listDatasets(zpool) {
    if (!zpool) {
      throw new Error('ZFS pool name is required');
    }

    return this.executeCommand('list_datasets', { zpool }, 'List datasets');
  }

  /**
   * Create ZFS dataset
   * @param {string} zpool - ZFS pool name
   * @param {string} name - Dataset name
   * @param {string} usageType - Usage type (image, writeback, games)
   * @returns {Promise<Object>}
   */
  async createZfsDataset(zpool, name, usageType) {
    if (!zpool || !name || !usageType) {
      throw new Error('Pool name, dataset name, and usage type are required');
    }

    return this.executeCommand(
      'create_zfs_dataset',
      { zpool, name, usageType },
      'Create ZFS dataset'
    );
  }

  /**
   * Delete ZFS dataset
   * @param {string} name - Dataset name
   * @returns {Promise<Object>}
   */
  async deleteZfsDataset(name) {
    if (!name) {
      throw new Error('Dataset name is required');
    }

    return this.executeCommand('delete_zfs_dataset', { name }, 'Delete ZFS dataset');
  }

  /**
   * Rename ZFS dataset
   * @param {string} oldName - Current dataset name
   * @param {string} newName - New dataset name
   * @returns {Promise<Object>}
   */
  async renameZfsDataset(oldName, newName) {
    if (!oldName || !newName) {
      throw new Error('Old and new dataset names are required');
    }

    return this.executeCommand(
      'rename_zfs_dataset',
      { oldName, newName },
      'Rename ZFS dataset'
    );
  }
}

// ========== LICENSE SERVICE ==========
/**
 * License management API service
 */
export class LicenseService extends BaseApiService {
  /**
   * Activate license
   * @param {string} licenseKey - License key
   * @returns {Promise<Object>}
   */
  async activateLicense(licenseKey) {
    if (!licenseKey) {
      throw new Error('License key is required');
    }

    return this.executeCommand('activate_license', { licenseKey }, 'Activate license');
  }

  /**
   * Get license information
   * @returns {Promise<Object>}
   */
  async getLicenseInfo() {
    return this.executeCommand('get_license_info', {}, 'Get license info');
  }
}

// ========== SYSTEM SERVICE ==========
/**
 * System information and utilities API service
 */
export class SystemService extends BaseApiService {
  /**
   * Get server information
   * @returns {Promise<Object>}
   */
  async getServerInfo() {
    return this.executeCommand('get_server_info', {}, 'Get server info');
  }

  /**
   * List system disks
   * @returns {Promise<Object[]>}
   */
  async listDisks() {
    return this.executeCommand('list_disks', {}, 'List disks');
  }

  /**
   * Get RAM usage
   * @returns {Promise<Object>}
   */
  async getRamUsage() {
    return this.executeCommand('get_ram_usage', {}, 'Get RAM usage');
  }

  /**
   * Clear RAM cache
   * @returns {Promise<Object>}
   */
  async clearRamCache() {
    return this.executeCommand('clear_ram_cache', {}, 'Clear RAM cache');
  }

  /**
   * Get service logs
   * @param {string} unit - Service unit name
   * @param {number} lines - Number of lines to retrieve
   * @returns {Promise<string>}
   */
  async getServiceLogs(unit, lines = 200) {
    if (!unit) {
      throw new Error('Service unit name is required');
    }

    return this.executeCommand('get_service_logs', { unit, lines }, 'Get service logs');
  }

  /**
   * Get application logs
   * @returns {Promise<string>}
   */
  async getLogs() {
    return this.executeCommand('get_logs', {}, 'Get application logs');
  }

  /**
   * Clear application logs
   * @returns {Promise<Object>}
   */
  async clearLogs() {
    return this.executeCommand('clear_logs', {}, 'Clear logs');
  }
}

// ========== CONFIGURATION SERVICE ==========
/**
 * Configuration management API service
 */
export class ConfigService extends BaseApiService {
  /**
   * Read configuration
   * @returns {Promise<Object>}
   */
  async readConfig() {
    return this.executeCommand('read_config', {}, 'Read configuration');
  }

  /**
   * Save configuration
   * @param {Object} config - Configuration object
   * @returns {Promise<Object>}
   */
  async saveConfig(config) {
    if (!config) {
      throw new Error('Configuration object is required');
    }

    return this.executeCommand('save_config', config, 'Save configuration');
  }
}

// ========== ZFS SERVICE ==========
/**
 * ZFS-specific operations API service
 */
export class ZfsService extends BaseApiService {
  /**
   * Get ZFS arc statistics
   * @returns {Promise<Object>}
   */
  async getZfsArcstat() {
    return this.executeCommand('get_zfs_arcstat', {}, 'Get ZFS arc statistics');
  }

  /**
   * Check if ZFS pool exists
   * @param {string} poolName - Pool name
   * @returns {Promise<boolean>}
   */
  async zfsPoolExists(poolName) {
    if (!poolName) {
      throw new Error('Pool name is required');
    }

    return this.executeCommand('zfs_pool_exists', { poolName }, 'Check ZFS pool existence');
  }

  /**
   * Get ZFS pool list
   * @returns {Promise<Object[]>}
   */
  async getZpoolList() {
    return this.executeCommand('get_zpool_list', {}, 'Get ZFS pool list');
  }
}

// ========== SINGLETON EXPORTS ==========
/**
 * Centralized API service instances
 */
export const authService = new AuthService();
export const clientService = new ClientService();
export const serviceService = new ServiceService();
export const imageService = new ImageService();
export const diskService = new DiskService();
export const licenseService = new LicenseService();
export const systemService = new SystemService();
export const configService = new ConfigService();
export const zfsService = new ZfsService();

/**
 * Default export - all services
 */
export default {
  authService,
  clientService,
  serviceService,
  imageService,
  diskService,
  licenseService,
  systemService,
  configService,
  zfsService
};