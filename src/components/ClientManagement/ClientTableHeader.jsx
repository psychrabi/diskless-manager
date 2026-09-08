import { TableHead } from "@/components/ui/table";
import { cn } from "@/lib/utils";

const headerCellClass = (baseClassName = "", fixed) => {
  return cn(baseClassName, fixed && "bg-card");
};

const ClientTableHeader = ({ fixed = false }) => {
  return (
    <tr
      className={cn(
        "border-b border-border",
        fixed && "z-10 w-full text-center bg-card shadow-sm"
      )}
    >
      <TableHead className={headerCellClass("w-30", fixed)}>Name</TableHead>
      <TableHead className={headerCellClass("hidden md:table-cell w-40", fixed)}>
        MAC Address
      </TableHead>
      <TableHead className={headerCellClass("w-36", fixed)}>IP Address</TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell w-24", fixed)}>
        Read Speed (MB/s)
      </TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell", fixed)}>
        Total Read
      </TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell w-24", fixed)}>
        Write Speed (MB/s)
      </TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell", fixed)}>
        Total Write
      </TableHead>
      <TableHead className={headerCellClass("hidden xl:table-cell", fixed)}>
        Image
      </TableHead>
      <TableHead className={headerCellClass("", fixed)}>Mode</TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell", fixed)}>
        Uptime
      </TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>Actions</TableHead>
    </tr>
  );
};

export default ClientTableHeader;
