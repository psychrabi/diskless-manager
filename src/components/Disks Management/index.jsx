import { useAppStore } from '@/store/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { HardDrive, PlusCircle } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Button, Card } from '../ui';
import DiskFormModal from '@/components/Disks Management/DiskFormModal';
import DiskTable from './DiskTable';
import { useNotification } from '@/contexts/notification';

export default function DisksManagement() {
  const { fetchData } = useAppStore();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [zpools, setZpools] = useState([]);
  const [datasets, setDatasets] = useState([]);
  const [selectedPool, setSelectedPool] = useState('');
  const { showNotification } = useNotification();

  const handleDiskFormModalOpen = useCallback(() => {
    setIsModalOpen(true)
  }, [])

  const fetchZpools = useCallback(async () => {
    try {
      const res = await invoke('list_zpools');
      setZpools(res || []);
      if ((res || []).length > 0 && !selectedPool) {
        setSelectedPool(res[0]);
      }
    } catch (e) {
      showNotification('error', 'Failed to list ZFS pools', e.message || 'An unknown error occurred');
      console.error(String(e));
    }
  }, [selectedPool, showNotification]);

  const fetchDatasets = useCallback(async (pool) => {
    if (!pool) {
      setDatasets([]);
      return;
    }
    try {
      const res = await invoke('list_datasets', { zpool: pool });
      setDatasets(res || []);
    } catch (e) {
      showNotification('error', 'Failed to list datasets', e.message || 'An unknown error occurred');
      console.error(String(e));
    }
  }, [showNotification]);

  useEffect(() => {
    const getZpools = async () => {
      await fetchZpools();
    };
    getZpools();
  }, [fetchZpools]);

  useEffect(() => {
    if (selectedPool) {
      const getDatasets = async () => {
        await fetchDatasets(selectedPool);
      };
      getDatasets();
    }
  }, [selectedPool, fetchDatasets]);


  return (
    <Card title="Disk Management" icon={HardDrive} className="bg-base-300" actions={
      <Button variant="primary" onClick={() => handleDiskFormModalOpen()} icon={PlusCircle} >
        Add Disk
      </Button>
    } >
      <div className="min-h-[calc(100vh-15rem)]">
        <DiskTable disks={datasets} />
      </div>
      {isModalOpen && <DiskFormModal zpools={zpools} isOpen={isModalOpen} setIsOpen={setIsModalOpen} refresh={fetchData} />}
    </Card>
  );
}