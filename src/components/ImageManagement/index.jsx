import { HardDrive, PlusCircle } from 'lucide-react';
import { useState } from 'react';
import { useAppStore } from '../../store/useAppStore';
import { Button, Card } from '../ui';
import { ImagesList } from './ImagesList';
import CreateImageModal from './CreateImageModal';

const ImageManagement = () => {
  const { masters } = useAppStore();
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false)

  const handleCreateImage = () => setOpenImageCreateModal(true)

  return (
    <div className="space-y-6">
      <Card title="Image Management" icon={HardDrive} actions={
        <Button variant="primary" onClick={() => handleCreateImage()} icon={PlusCircle}>Create Image</Button>
      }>
        <div className="space-y-6">
          <ImagesList masters={masters} />
        </div>
      </Card>
      {openImageCreateModal && <CreateImageModal openImageCreateModal={openImageCreateModal} setOpenImageCreateModal={setOpenImageCreateModal} />}
    </div>
  );
};

export default ImageManagement;
