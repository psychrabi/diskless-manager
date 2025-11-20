import React from 'react';

export const Table = ({ children, className = '' }) => (
  <div className={`w-full overflow-x-auto ${className}`}>
    <table className="table w-full">{children}</table>
  </div>
);

export const TableHeader = ({ children, className = '' }) => (
  <thead className={`[&_tr]:border-b border-base-200 ${className}`}>{children}</thead>
);

export const TableBody = ({ children, className = '' }) => (
  <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>
);

export const TableRow = ({ children, className = '', ...props }) => (
  <tr className={`border-b border-base-200 transition-colors hover:bg-base-200/50 ${className}`} {...props}>
    {children}
  </tr>
);

export const TableHead = ({ children, className = '' }) => (
  <th className={`h-12 px-4 align-middle font-medium text-base-content/70 ${className}`}>{children}</th>
);

export const TableCell = ({ children, className = '' }) => (
  <td className={`p-4 align-middle ${className}`}>{children}</td>
);
