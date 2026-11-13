import { TableHead, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

const headerCellClass = (baseClassName = "", fixed) => {
  return cn(
    "h-9 px-3 text-[10px] font-semibold tracking-[0.08em] uppercase text-muted-foreground",
    baseClassName,
    fixed && "bg-card"
  );
};

const ClientTableHeader = ({ fixed = false }) => {
  return (
    <TableRow
      className={cn(
        "hidden border-border/70 bg-muted/25 hover:bg-transparent xl:table-row",
        fixed && "z-10 bg-card shadow-sm"
      )}
    >
      <TableHead className={headerCellClass("min-w-48", fixed)}>Name</TableHead>
      <TableHead className={headerCellClass("hidden text-center xl:table-cell", fixed)}>
        MAC Address
      </TableHead>
      <TableHead className={headerCellClass("hidden text-center xl:table-cell", fixed)}>
        IP Address
      </TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>
        I/O (MB/s)
      </TableHead>
      <TableHead className={headerCellClass("hidden text-center lg:table-cell max-2xl:hidden", fixed)}>
        I/O (Total)
      </TableHead>
      <TableHead className={headerCellClass("hidden text-center xl:table-cell", fixed)}>
        Image
      </TableHead>
      <TableHead className={headerCellClass("text-center", fixed)}>Mode</TableHead>
      <TableHead className={headerCellClass("hidden text-center lg:table-cell max-2xl:hidden", fixed)}>
        Uptime
      </TableHead>
      <TableHead className={headerCellClass("sticky right-0 z-20 w-32 bg-card text-center", fixed)}>
        Actions
      </TableHead>
    </TableRow>
  );
};

export default ClientTableHeader;
