import React, { forwardRef } from 'react';
import PropTypes from 'prop-types';

export const Table = forwardRef(({ children, className = '', 'aria-label': ariaLabel, ...props }, ref) => (
  <div className={`w-full overflow-x-auto ${className}`} ref={ref} {...props}>
    <table className="table w-full" aria-label={ariaLabel}>{children}</table>
  </div>
));
Table.displayName = 'Table';

Table.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
  'aria-label': PropTypes.string,
};

export const TableHeader = forwardRef(({ children, className = '', ...props }, ref) => (
  <thead className={`[&_tr]:border-b border-base-200 ${className}`} ref={ref} {...props}>{children}</thead>
));
TableHeader.displayName = 'TableHeader';

TableHeader.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableBody = forwardRef(({ children, className = '', ...props }, ref) => (
  <tbody className={`[&_tr:last-child]:border-0 ${className}`} ref={ref} {...props}>{children}</tbody>
));
TableBody.displayName = 'TableBody';

TableBody.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableRow = forwardRef(({ children, className = '', ...props }, ref) => (
  <tr className={`border-b border-base-200 transition-colors hover:bg-base-200/50 ${className}`} ref={ref} {...props}>
    {children}
  </tr>
));
TableRow.displayName = 'TableRow';

TableRow.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableHead = forwardRef(({ children, className = '', scope = 'col', ...props }, ref) => (
  <th scope={scope} className={`h-12 px-4 align-middle font-medium text-base-content/70 ${className}`} ref={ref} {...props}>{children}</th>
));
TableHead.displayName = 'TableHead';

TableHead.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
  scope: PropTypes.oneOf(['col', 'row', 'colgroup', 'rowgroup']),
};

export const TableCell = forwardRef(({ children, className = '', ...props }, ref) => (
  <td className={`p-4 align-middle ${className}`} ref={ref} {...props}>{children}</td>
));
TableCell.displayName = 'TableCell';

TableCell.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};
