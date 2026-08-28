import { TableHead } from "@/components/ui";

const headerCellClass = (baseClassName = "", fixed) => {
  const fixedClassName = fixed ? " bg-base-100" : "";
  return `${baseClassName}${fixedClassName}`.trim();
};

const ClientTableHeader = ({ fixed = false }) => {
  return (
    <tr
      className={`border-b border-base-200 ${fixed ? "bg-base-100 shadow-sm z-10 w-full text-center" : ""}`}
    >
      <TableHead className={headerCellClass("w-30", fixed)}>Name</TableHead>
      <TableHead className={headerCellClass("hidden md:table-cell w-40", fixed)}>
        MAC Address
      </TableHead>
      <TableHead className={headerCellClass("w-36", fixed)}>IP Address</TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell w-20", fixed)}>
        Network ↓ <span className="hidden">(MB/s)</span>
      </TableHead>
      <TableHead className={headerCellClass("hidden lg:table-cell w-20", fixed)}>
        Network ↑ <span className="hidden">(MB/s)</span>
      </TableHead>
      <TableHead className={headerCellClass("hidden xl:table-cell w-20", fixed)}>
        iSCSI ↓ <span className="hidden">(MB/s)</span>
      </TableHead>
      <TableHead className={headerCellClass("hidden xl:table-cell w-20", fixed)}>
        iSCSI ↑ <span className="hidden">(MB/s)</span>
      </TableHead>
      <TableHead className={headerCellClass("hidden xl:table-cell", fixed)}>
        Image
      </TableHead>
      <TableHead className={headerCellClass("hidden 2xl:table-cell", fixed)}>
        Restore Point
      </TableHead>
      <TableHead className={headerCellClass("hidden 2xl:table-cell", fixed)}>
        Boot disk
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
