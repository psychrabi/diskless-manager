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
    <Card className="border-dashed shadow-none">
      <CardContent className="flex flex-col items-center gap-4 px-6 py-14 text-center">
        <div className="relative flex size-16 items-center justify-center rounded-2xl border border-primary/15 bg-primary/10">
          <Laptop className="size-7 text-primary" />
          <span className="absolute -right-1 -bottom-1 size-3 rounded-full border-2 border-card bg-success" />
        </div>
        <div className="flex flex-col gap-1.5">
          <CardTitle className="text-lg font-semibold">
            No clients yet
          </CardTitle>
          <CardDescription className="mx-auto max-w-md">
            Add your first diskless client with its MAC address. It will register the next time it network boots.
          </CardDescription>
        </div>
        <Button variant="default" onClick={handleClientFormModalOpen}>
          <PlusCircle data-icon="inline-start" />
          Add your first client
        </Button>
      </CardContent>
    </Card>
  );
}

export default ClientHero;
