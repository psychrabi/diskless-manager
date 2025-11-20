import { useZfs } from '@/hooks/useZfs';
import { useAppStore } from '@/store/useAppStore';
import { HardDrive, PlusCircle } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Button, Card } from '../ui';
import DiskFormModal from './DiskFormModal';
import DiskTable from './DiskTable';

export default function DisksManagement() {
  const { fetchData } = useAppStore();
  const zpools = useAppStore((state) => state.zpools);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedPool, setSelectedPool] = useState('');
  const { datasets, fetchDatasets } = useZfs();

  const handleDiskFormModalOpen = useCallback(() => {
    setIsModalOpen(true)
  }, [])

  // Set default pool when zpools are loaded
  useEffect(() => {
    if (zpools.length > 0 && !selectedPool) {
      setSelectedPool(zpools[0]);
    }
  }, [zpools, selectedPool]);

  useEffect(() => {
    if (selectedPool) {
      fetchDatasets(selectedPool);
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