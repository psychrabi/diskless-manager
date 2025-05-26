import React from 'react';

export const Table = ({ data, columns, renderCell }) => (
  <div className="overflow-x-auto">
    <table className="table w-full">
      <thead>
        <tr>
          {columns.map(column => (
            <th key={column.key} className={column.width}>{column.label}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {data.map(row => (
          <tr key={row.id}>
            {columns.map(column => (
              <td key={column.key}>{renderCell(row, column)}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  </div>
);
