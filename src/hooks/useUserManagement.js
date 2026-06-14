/**
 * User Management Hook
 * Provides functions for managing users with authentication
 */

import { useState, useCallback } from 'react';
import { listUsers as listUsersApi, getUser as getUserApi, createUser as createUserApi, updateUser as updateUserApi, updateUserPassword as updateUserPasswordApi, deleteUser as deleteUserApi } from '@/api/modules/users';

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
      const users = await listUsersApi();
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
      const user = await getUserApi(userId);
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
        const user = await createUserApi({
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
        const user = await updateUserApi(userId, updates);
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
        const result = await updateUserPasswordApi(userId, password);
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
        const result = await deleteUserApi(userId);
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
