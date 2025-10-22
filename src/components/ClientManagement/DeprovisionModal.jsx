import React, { useState, useEffect, useCallback } from 'react';
import { useDeprovisioning } from '../../hooks/useDeprovisioning';
import { Button, Modal, Input, Card } from '../ui';

export const DeprovisionModal = ({
  isOpen,
  onClose,
  client = null,
  onSuccess
}) => {
  const [mac, setMac] = useState(client?.mac || '');
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

  const loadStatus = useCallback(async (signal) => { // Accept signal
    if (!mac) return;

    try {
      const result = await getDeprovisionStatus(mac);
      if (signal.aborted) return; // Check if aborted

      if (result.success) {
        setStatus(result.data);
        setShowStatus(true);
      }
    } catch (error) {
      if (signal.aborted) return; // Check if aborted
      console.error("Failed to load deprovision status:", error);
    }
  }, [mac, getDeprovisionStatus]);




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
    setForce(false);
    setKeepZfs(false);
    setDryRun(false);
    setStatus(null);
    setShowStatus(false);
    clearError();
    onClose();
  };

  const canDeprovision = mac && !loading;

  useEffect(() => {
    if (isOpen && mac) {
      const abortController = new AbortController();
      const signal = abortController.signal;

      const fetchStatus = async () => {
        await loadStatus(signal);
      };
      fetchStatus();

      return () => {
        abortController.abort();
      };
    }
  }, [isOpen, mac, loadStatus]);

  return (
    <Modal isOpen={isOpen} onClose={handleClose} title="Deprovision Client">
      <div className="space-y-4">
        {error && (
          <div className="alert alert-error">
            <span className="text-sm">{error}</span>
          </div>
        )}

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-base-content mb-1">
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
              <h4 className="font-medium text-base-content mb-3">Client Status</h4>
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span>ZFS Clone:</span>
                  <span className={status.zfs_clone_exists ? 'text-success' : 'text-base-content/60'}>
                    {status.zfs_clone_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>iSCSI Target:</span>
                  <span className={status.iscsi_target_exists ? 'text-success' : 'text-base-content/60'}>
                    {status.iscsi_target_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>PXE Config:</span>
                  <span className={status.pxe_config_exists ? 'text-success' : 'text-base-content/60'}>
                    {status.pxe_config_exists ? 'Exists' : 'Not found'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Client Online:</span>
                  <span className={status.client_online ? 'text-error' : 'text-base-content/60'}>
                    {status.client_online ? 'Online' : 'Offline'}
                  </span>
                </div>
              </div>
            </Card>
          )}

          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="force"
                checked={force}
                onChange={(e) => setForce(e.target.checked)}
                className="checkbox checkbox-primary"
              />
              <label htmlFor="force" className="text-sm text-base-content">
                Force deprovisioning (even if client is online)
              </label>
            </div>

            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="keep-zfs"
                checked={keepZfs}
                onChange={(e) => setKeepZfs(e.target.checked)}
                className="checkbox checkbox-primary"
              />
              <label htmlFor="keep-zfs" className="text-sm text-base-content">
                Keep ZFS clone (don't destroy storage)
              </label>
            </div>

            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="dry-run"
                checked={dryRun}
                onChange={(e) => setDryRun(e.target.checked)}
                className="checkbox checkbox-primary"
              />
              <label htmlFor="dry-run" className="text-sm text-base-content">
                Dry run (show what would be done)
              </label>
            </div>
          </div>

          {status?.client_online && !force && (
            <div className="alert alert-warning">
              <span className="text-sm">
                ⚠️ Client appears to be online. Use "Force deprovisioning" to proceed anyway.
              </span>
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