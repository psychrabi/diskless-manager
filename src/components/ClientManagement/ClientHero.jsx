import { PlusCircle, Laptop } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";

function ClientHero({ handleClientFormModalOpen }) {
  return (
    <Card className="py-12 text-center">
      <CardContent className="flex flex-col items-center gap-4">
        <span className="flex size-20 items-center justify-center rounded-full bg-muted">
          <Laptop className="size-10 text-muted-foreground/60" />
        </span>
        <div className="flex flex-col gap-2">
          <CardTitle className="text-lg font-semibold">
            No Clients Found
          </CardTitle>
          <CardDescription className="mx-auto max-w-md">
            Get started by adding your first diskless boot client. You'll need
            the client's MAC address and desired IP configuration.
          </CardDescription>
        </div>
        <div className="flex flex-col justify-center gap-3 sm:flex-row">
          <Button variant="default" onClick={handleClientFormModalOpen}>
            <PlusCircle data-icon="inline-start" />
            Add Your First Client
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

export default ClientHero;
