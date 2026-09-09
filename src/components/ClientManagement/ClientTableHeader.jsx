import { TableHead, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

const headerCellClass = (baseClassName = "", fixed) => {
  return cn(baseClassName, fixed && "bg-card");
};

const ClientTableHeader = ({ fixed = false }) => {
  return (
    <TableRow
      className={cn(
        "border-border hover:bg-transparent",
        fixed && "z-10 bg-card shadow-sm"
      )}
    >
      <TableHead className={headerCellClass("text-center", fixed)}>Client</TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>
        Throughput (MB/s)
      </TableHead>
      <TableHead className={headerCellClass("hidden text-center lg:table-cell", fixed)}>
        Total I/O
      </TableHead>
      <TableHead className={headerCellClass("hidden text-center xl:table-cell", fixed)}>
        Image
      </TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>Mode</TableHead>
      <TableHead className={headerCellClass("hidden text-center lg:table-cell", fixed)}>
        Uptime
      </TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>Actions</TableHead>
    </TableRow>
  );
};

export default ClientTableHeader;
