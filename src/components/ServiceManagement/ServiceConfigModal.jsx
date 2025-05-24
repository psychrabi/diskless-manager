import React, { useState } from 'react';
import { Button } from '../ui';
import { Modal } from '../ui';

const ServiceConfigModal = ({ open, setOpen, title, loading, content }) => {

  return (
    <Modal isOpen={open} onClose={() => setOpen(false)} title={title} size="2xl">
      {loading ? (
        <div className="flex justify-center items-center h-40">
          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
        </div>
      ) : (
        <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-md text-xs overflow-auto max-h-[70vh]">
          <code>{content}</code>
        </pre>
      )}
      <div className="mt-4 flex justify-end">
        <Button variant="outline" onClick={() => setOpen(false)}>Close</Button>
      </div>
    </Modal>
  )
};

export default ServiceConfigModal;