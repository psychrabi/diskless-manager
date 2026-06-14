import { Laptop } from "lucide-react";
import ClientTableHeader from "./ClientTableHeader";

const ClientTableEmptyState = () => {
  return (
    <div className="p-4">
      <table className="table w-full">
        <thead>
          <ClientTableHeader />
        </thead>
        <tbody>
          <tr>
            <td colSpan="11">
              <div className="flex flex-col items-center justify-center py-12 text-base-content/50">
                <Laptop className="h-12 w-12 mb-3 opacity-40" />
                <p className="text-sm font-medium">No clients configured</p>
                <p className="text-xs mt-1">Add your first client using the button above</p>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
};

export default ClientTableEmptyState;
