import { Laptop } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { CardDescription } from "@/components/ui/card";
import ClientTableHeader from "./ClientTableHeader";

const ClientTableEmptyState = () => {
  return (
    <div className="p-4">
      <Table>
        <TableHeader>
          <ClientTableHeader />
        </TableHeader>
        <TableBody>
          <TableRow className="border-0 hover:bg-transparent">
            <TableCell colSpan="7">
              <div className="flex flex-col items-center justify-center py-12 text-muted-foreground/60">
                <Laptop className="mb-3 size-12 opacity-40" />
                <p className="text-sm font-medium">No clients configured</p>
                <CardDescription className="mt-1 text-xs">Add your first client using the button above</CardDescription>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </div>
  );
};

export default ClientTableEmptyState;
