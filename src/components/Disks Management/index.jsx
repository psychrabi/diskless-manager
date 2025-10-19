import { useAppStore } from '@/store/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { HardDrive, PlusCircle } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Button, Card } from '../ui';
import DiskFormModal from './DiskFormModal';
import DiskTable from './DiskTable';

export default function DisksManagement() {
  const { fetchData } = useAppStore();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [zpools, setZpools] = useState([]);
  const [datasets, setDatasets] = useState([]);
  const [selectedPool, setSelectedPool] = useState('');

  const handleDiskFormModalOpen = useCallback(() => {
    setIsModalOpen(true)
  }, [])

  async function fetchZpools() {
    try {
      const res = await invoke('list_zpools');
      setZpools(res || []);
      if ((res || []).length > 0 && !selectedPool) {
        setSelectedPool(res[0]);
      }
    } catch (e) {
      console.error(String(e));
    }
  }

  async function fetchDatasets(pool) {
    if (!pool) {
      setDatasets([]);
      return;
    }
    try {
      const res = await invoke('list_datasets', { zpool: pool });
      setDatasets(res || []);
    } catch (e) {
      console.error(String(e));
    }
  }

  useEffect(() => { fetchZpools(); }, []);

  useEffect(() => {
    if (selectedPool) {
      fetchDatasets(selectedPool);
    }
  }, [selectedPool]);


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