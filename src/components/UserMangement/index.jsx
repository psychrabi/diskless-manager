import { Button, Card } from '@/components/ui';
import { useUserManagement } from '@/hooks/useUserManagement';
import { useToastStore } from '@/store/useToastStore';
import { Key, Pencil, Plus, Trash2, Users } from 'lucide-react';
import { useEffect, useState } from 'react';

import { useConfirm } from "@/contexts/confirmDialog";
import ChangePasswordModal from './ChangePasswordModal';
import CreateUserModal from './CreateUserModal';
import EditUserModal from './EditUserModal';

export default function UserManagement() {
  const [users, setUsers] = useState([]);
  const [selectedUser, setSelectedUser] = useState(null);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showEditModal, setShowEditModal] = useState(false);
  const [showPasswordModal, setShowPasswordModal] = useState(false);

  const { listUsers, deleteUser, loading } = useUserManagement();
  const { success, error } = useToastStore();
  const { confirm } = useConfirm();

  const loadUsers = async () => {
    try {
      const data = await listUsers();
      setUsers(data);
    } catch (err) {
      error('Load Users', err.message || 'Failed to load users');
    }
  };

  useEffect(() => {
    loadUsers();
  }, []);

  const handleCreateUser = () => {
    setShowCreateModal(true);
  };

  const handleEditUser = (user) => {
    setSelectedUser(user);
    setShowEditModal(true);
  };

  const handleChangePassword = (user) => {
    setSelectedUser(user);
    setShowPasswordModal(true);
  };

  const handleDeleteUser = async (user) => {
    const confirmed = await confirm(
      'Delete User',
      `Are you sure you want to delete user "${user.username}"? This action cannot be undone.`
    );

    if (!confirmed) return;

    try {
      await deleteUser(user.id);
      success('Delete User', `User "${user.username}" deleted successfully`);
      loadUsers();
    } catch (err) {
      error('Delete User', err.message || 'Failed to delete user');
    }
  };

  const handleModalClose = () => {
    setShowCreateModal(false);
    setShowEditModal(false);
    setShowPasswordModal(false);
    setSelectedUser(null);
    loadUsers();
  };

  return (
    <>
      <Card
        title="User Management"
        subtitle="Manage system users and their roles"
        icon={Users}
        className="bg-base-300"
        actions={ <Button
            variant="primary"
            onClick={handleCreateUser}
            icon={Plus}
            disabled={loading}
          >
            Create User
          </Button>}
      >   

    <div className="bg-base-100 rounded-lg h-[calc(100vh-20rem)] w-full border border-base-200">
          <table className="table table-zebra w-full">
            <thead>
              <tr>
                <th>Username</th>
                <th>Role</th>
                <th className="text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {loading && users.length === 0 ? (
                <tr>
                  <td colSpan="3" className="text-center py-8">
                    <span className="loading loading-spinner loading-md"></span>
                  </td>
                </tr>
              ) : users.length === 0 ? (
                <tr>
                  <td colSpan="3" className="text-center py-8 text-base-content/60">
                    No users found
                  </td>
                </tr>
              ) : (
                users.map((user) => (
                  <tr key={user.id}>
                    <td>
                      <div className="font-medium">{user.username}</div>
                    </td>
                    <td>
                      <span
                        className={`badge ${
                          user.role === 'admin'
                            ? 'badge-primary'
                            : 'badge-secondary'
                        }`}
                      >
                        {user.role}
                      </span>
                    </td>
                    <td>
                      <div className="flex justify-end gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleEditUser(user)}
                          icon={Pencil}
                          title="Edit user"
                        />
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleChangePassword(user)}
                          icon={Key}
                          title="Change password"
                        />
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteUser(user)}
                          icon={Trash2}
                          title="Delete user"
                          className="text-error hover:bg-error/10"
                        />
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </Card>

      {showCreateModal && (
        <CreateUserModal
          isOpen={showCreateModal}
          onClose={handleModalClose}
        />
      )}

      {showEditModal && selectedUser && (
        <EditUserModal
          isOpen={showEditModal}
          onClose={handleModalClose}
          user={selectedUser}
        />
      )}

      {showPasswordModal && selectedUser && (
        <ChangePasswordModal
          isOpen={showPasswordModal}
          onClose={handleModalClose}
          user={selectedUser}
        />
      )}
    </>
  );
}
