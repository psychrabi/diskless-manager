/**
 * User Management Hook
 * Provides functions for managing users with authentication
 */

import { useState, useCallback } from 'react';
import * as api from '@/api/commands';

export function useUserManagement() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  /**
   * List all users
   */
  const listUsers = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const users = await api.listUsers();
      return users;
    } catch (err) {
      setError(err.message || 'Failed to list users');
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Get a specific user by ID
   */
  const getUser = useCallback(async (userId) => {
    setLoading(true);
    setError(null);
    try {
      const user = await api.getUser(userId);
      return user;
    } catch (err) {
      setError(err.message || 'Failed to get user');
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Create a new user
   */
  const createUser = useCallback(
    async (username, password, role = 'user') => {
      setLoading(true);
      setError(null);
      try {
        const user = await api.createUser({
          username,
          password,
          role,
        });
        return user;
      } catch (err) {
        setError(err.message || 'Failed to create user');
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  /**
   * Update user details
   */
  const updateUser = useCallback(
    async (userId, updates) => {
      setLoading(true);
      setError(null);
      try {
        const user = await api.updateUser(userId, updates);
        return user;
      } catch (err) {
        setError(err.message || 'Failed to update user');
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  /**
   * Update user password
   */
  const updateUserPassword = useCallback(
    async (userId, password) => {
      setLoading(true);
      setError(null);
      try {
        const result = await api.updateUserPassword(userId, password);
        return result;
      } catch (err) {
        setError(err.message || 'Failed to update password');
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  /**
   * Delete a user
   */
  const deleteUser = useCallback(
    async (userId) => {
      setLoading(true);
      setError(null);
      try {
        const result = await api.deleteUser(userId);
        return result;
      } catch (err) {
        setError(err.message || 'Failed to delete user');
        throw err;
      } finally {
        setLoading(false);
      }
    },
    []
  );

  return {
    loading,
    error,
    listUsers,
    getUser,
    createUser,
    updateUser,
    updateUserPassword,
    deleteUser,
  };
}
