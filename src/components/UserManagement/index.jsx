import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
} from "@/components/ui/table";
import { Spinner } from "@/components/ui/spinner";
import { Button, Card, PageHeader } from '@/components/ui';
import { useUserManagement } from '@/hooks/useUserManagement';
import { useToastStore } from '@/store/useToastStore';
import { Key, Pencil, Plus, Trash2, Users } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';

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
  const confirm = useConfirm();

  const loadUsers = useCallback(async () => {
    try {
      const data = await listUsers();
      setUsers(data);
    } catch (err) {
      error('Load Users', err.message || 'Failed to load users');
    }
  }, [error, listUsers]);

  useEffect(() => {
    const timer = setTimeout(() => {
      void loadUsers();
    }, 0);
    return () => clearTimeout(timer);
  }, [loadUsers]);

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
    const confirmed = await confirm({
      title: "Delete User",
      description: `Are you sure you want to delete user "${user.username}"? This action cannot be undone.`,
      confirmText: "Delete",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });

    if (!confirmed) return;

    try {
      await deleteUser(user.id);
      success('Delete User', `User "${user.username}" deleted successfully`);
      void loadUsers();
    } catch (err) {
      error('Delete User', err.message || 'Failed to delete user');
    }
  };

  const handleModalClose = () => {
    setShowCreateModal(false);
    setShowEditModal(false);
    setShowPasswordModal(false);
    setSelectedUser(null);
    void loadUsers();
  };

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="User Management"
        description="Manage system users and their roles."
      />
      <Card
        title="Users"
        subtitle={`${users.length} user${users.length === 1 ? "" : "s"}`}
        icon={Users}
        actions={ <Button
            variant="primary"
            onClick={handleCreateUser}
            icon={Plus}
            disabled={loading}
          >
            Create User
          </Button>}
      >
        <div className="bg-background rounded-lg max-h-[70vh] w-full border border-border overflow-auto">
          <Table className="w-full">
            <TableHeader>
              <TableRow>
                <TableHead>Username</TableHead>
                <TableHead>Role</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {loading && users.length === 0 ? (
                <TableRow>
                  <TableCell colSpan="3" className="text-center py-8">
                    <Spinner />
                  </TableCell>
                </TableRow>
              ) : users.length === 0 ? (
                <TableRow>
                  <TableCell colSpan="3" className="text-center py-8 text-muted-foreground">
                    No users found
                  </TableCell>
                </TableRow>
              ) : (
                users.map((user) => (
                  <TableRow key={user.id}>
                    <TableCell>
                      <div className="font-medium">{user.username}</div>
                    </TableCell>
                    <TableCell>
                      <Badge variant={user.role === "admin" ? "default" : "secondary"}>
                        {user.role}
                      </Badge>
                    </TableCell>
                    <TableCell>
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
                          className="text-destructive hover:bg-destructive/10"
                        />
                      </div>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
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
    </div>
  );
}
