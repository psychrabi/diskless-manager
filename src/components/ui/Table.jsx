import React from 'react';
import PropTypes from 'prop-types';

export const Table = ({ children, className = '' }) => (
  <div className={`w-full overflow-x-auto ${className}`}>
    <table className="table w-full">{children}</table>
  </div>
);

Table.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableHeader = ({ children, className = '' }) => (
  <thead className={`[&_tr]:border-b border-base-200 ${className}`}>{children}</thead>
);

TableHeader.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableBody = ({ children, className = '' }) => (
  <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>
);

TableBody.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableRow = ({ children, className = '', ...props }) => (
  <tr className={`border-b border-base-200 transition-colors hover:bg-base-200/50 ${className}`} {...props}>
    {children}
  </tr>
);

TableRow.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableHead = ({ children, className = '' }) => (
  <th className={`h-12 px-4 align-middle font-medium text-base-content/70 ${className}`}>{children}</th>
);

TableHead.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};

export const TableCell = ({ children, className = '' }) => (
  <td className={`p-4 align-middle ${className}`}>{children}</td>
);

TableCell.propTypes = {
  children: PropTypes.node,
  className: PropTypes.string,
};
