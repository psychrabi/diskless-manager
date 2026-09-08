import { Laptop } from "lucide-react";
import ClientTableHeader from "./ClientTableHeader";

const ClientTableEmptyState = () => {
  return (
    <div className="p-4">
      <table className="w-full">
        <thead className="[&_tr]:border-b">
          <ClientTableHeader />
        </thead>
        <tbody className="[&_tr:last-child]:border-0">
          <tr>
            <td colSpan="11" className="p-2 align-middle">
              <div className="flex flex-col items-center justify-center py-12 text-muted-foreground/60">
                <Laptop className="mb-3 size-12 opacity-40" />
                <p className="text-sm font-medium">No clients configured</p>
                <p className="mt-1 text-xs">Add your first client using the button above</p>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
};

export default ClientTableEmptyState;
