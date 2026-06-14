/**
 * User Management Hook
 * Provides functions for managing users with authentication
 */

import { useState, useCallback } from 'react';
import { listUsers as listUsersApi, createUser as createUserApi, updateUser as updateUserApi, updateUserPassword as updateUserPasswordApi, deleteUser as deleteUserApi } from '@/api/modules/users';

export function useUserManagement() {
  const [loading, setLoading] = useState(false);

  const listUsers = useCallback(async () => {
    setLoading(true);
    try {
      return await listUsersApi();
    } finally {
      setLoading(false);
    }
  }, []);

  const createUser = useCallback(
    async (username, password, role = 'user') => {
      setLoading(true);
      try {
        return await createUserApi({ username, password, role });
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const updateUser = useCallback(
    async (userId, updates) => {
      setLoading(true);
      try {
        return await updateUserApi(userId, updates);
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const updateUserPassword = useCallback(
    async (userId, password) => {
      setLoading(true);
      try {
        return await updateUserPasswordApi(userId, password);
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const deleteUser = useCallback(
    async (userId) => {
      setLoading(true);
      try {
        return await deleteUserApi(userId);
      } finally {
        setLoading(false);
      }
    },
    []
  );

  return {
    loading,
    listUsers,
    createUser,
    updateUser,
    updateUserPassword,
    deleteUser,
  };
}
