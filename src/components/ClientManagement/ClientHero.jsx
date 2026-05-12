import { PlusCircle, Laptop } from "lucide-react";
import React from "react";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";

function ClientHero({ handleClientFormModalOpen }) {
  return (
    <Card variant="outlined" className="text-center py-12">
      <div className="w-20 h-20 bg-base-200 rounded-full flex items-center justify-center text-4xl mb-6 mx-auto">
        <Laptop className="h-10 w-10 text-base-content/40" />
      </div>
      <h2 className="text-heading-lg font-semibold mb-3">No Clients Found</h2>
      <p className="text-body-md text-base-content/60 max-w-md mx-auto mb-6">
        Get started by adding your first diskless boot client. You'll need the
        client's MAC address and desired IP configuration.
      </p>
      <div className="flex flex-col sm:flex-row gap-3 justify-center">
        <Button
          variant="primary"
          onClick={handleClientFormModalOpen}
          icon={PlusCircle}
        >
          Add Your First Client
        </Button>
      </div>
    </Card>
  );
}

export default ClientHero;
