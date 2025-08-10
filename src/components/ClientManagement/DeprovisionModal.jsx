import React, { useState, useEffect } from 'react';
import { useDeprovisioning } from '../../hooks/useDeprovisioning';
import { Button, Modal, Input, Card } from '../ui';

export const DeprovisionModal = ({ 
  isOpen, 
  onClose, 
  client = null, 
  onSuccess 
}) => {
  const [mac, setMac] = useState('');
  const [force, setForce] = useState(false);
  const [keepZfs, setKeepZfs] = useState(false);
  const [dryRun, setDryRun] = useState(false);
  const [status, setStatus] = useState(null);
  const [showStatus, setShowStatus] = useState(false);

  const { 
    deprovisionClient, 
    deprovisionClientById, 
    getDeprovisionStatus, 
    loading, 
    error, 
    clearError 
  } = useDeprovisioning();

  useEffect(() => {
    if (client) {
      setMac(client.mac);
    }
  }, [client]);

  useEffect(() => {
    if (isOpen && mac) {
      loadStatus();
    }
  }, [isOpen, mac]);

  const loadStatus = async () => {
    if (!mac) return;
    
    const result = await getDeprovisionStatus(mac);
    if (result.success) {
      setStatus(result.data);
      setShowStatus(true);
    }
  };

  const handleDeprovision = async () => {
    if (!mac) return;

    clearError();
    
    const options = {
      force,
      keep_zfs: keepZfs,
      dry_run: dryRun,
    };

    let result;
    if (client) {
      result = await deprovisionClientById(client.id, options);
    } else {
      result = await deprovisionClient(mac, options);
    }

    if (result.success) {
      if (onSuccess) {
        onSuccess(result.data);
      }
      onClose();
    }
  };

  const handleClose = () => {
    setMac('');
    setForce(false);
    setKeepZfs(false);
    setDryRun(false);
    setStatus(null);
    setShowStatus(false);
    clearError();
    onClose();
  };

  const canDeprovision = mac && !loading;

  return (
    <Modal isOpen={isOpen} onClose={handleClose} title="Deprovision Client">
      <div className="space-y-4">
        {error && (
          <div className="bg-red-50 border border-red-200 rounded-md p-3">
            <p className="text-red-800 text-sm">{error}</p>
          </div>
        )}

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              MAC Address
            </label>
            <Input
              type="text"
              value={mac}
              onChange={(e) => setMac(e.target.value)}
              placeholder="00:11:22:33:44:55"
              disabled={!!client}
            />
          </div>

          {showStatus && status && (
            <Card className="p-4">
              <h4 className="font-medium text-gray-900 mb-3">Client Status</h4>
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span>ZFS Clone:</span>
                  <span className={status.zfs_clone_exists ? 'text-green-600' : 'text-gray-500'}>
                    {status.zfs_clone_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>iSCSI Target:</span>
                  <span className={status.iscsi_target_exists ? 'text-green-600' : 'text-gray-500'}>
                    {status.iscsi_target_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>PXE Config:</span>
                  <span className={status.pxe_config_exists ? 'text-green-600' : 'text-gray-500'}>
                    {status.pxe_config_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Client Online:</span>
                  <span className={status.client_online ? 'text-red-600' : 'text-gray-500'}>
                    {status.client_online ? 'Online' : 'Offline'}
                  </span>
                </div>
              </div>
            </Card>
          )}

          <div className="space-y-3">
            <div className="flex items-center">
              <input
                type="checkbox"
                id="force"
                checked={force}
                onChange={(e) => setForce(e.target.checked)}
                className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
              />
              <label htmlFor="force" className="ml-2 block text-sm text-gray-900">
                Force deprovisioning (even if client is online)
              </label>
            </div>

            <div className="flex items-center">
              <input
                type="checkbox"
                id="keep-zfs"
                checked={keepZfs}
                onChange={(e) => setKeepZfs(e.target.checked)}
                className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
              />
              <label htmlFor="keep-zfs" className="ml-2 block text-sm text-gray-900">
                Keep ZFS clone (don't destroy storage)
              </label>
            </div>

            <div className="flex items-center">
              <input
                type="checkbox"
                id="dry-run"
                checked={dryRun}
                onChange={(e) => setDryRun(e.target.checked)}
                className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
              />
              <label htmlFor="dry-run" className="ml-2 block text-sm text-gray-900">
                Dry run (show what would be done)
              </label>
            </div>
          </div>

          {status?.client_online && !force && (
            <div className="bg-yellow-50 border border-yellow-200 rounded-md p-3">
              <p className="text-yellow-800 text-sm">
                ⚠️ Client appears to be online. Use "Force deprovisioning" to proceed anyway.
              </p>
            </div>
          )}

          <div className="flex justify-end space-x-3 pt-4">
            <Button
              variant="outline"
              onClick={handleClose}
              disabled={loading}
            >
              Cancel
            </Button>
            <Button
              onClick={handleDeprovision}
              disabled={!canDeprovision}
              loading={loading}
              variant={dryRun ? "outline" : "destructive"}
            >
              {dryRun ? 'Simulate Deprovision' : 'Deprovision Client'}
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
};

export default DeprovisionModal;