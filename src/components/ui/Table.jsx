import React from 'react';

export const Table = ({ data, columns, renderCell }) => (
  <div className="overflow-x-auto">
    <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
      <thead className="bg-gray-50 dark:bg-gray-800">
        <tr>
          {columns.map(column => (
            <th
              key={column.key}
              scope="col"
              className={`px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider ${column.width}`}
            >
              {column.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody className="bg-white dark:bg-gray-800 divide-y divide-gray-200 dark:divide-gray-700">
        {data.map(row => (
          <tr key={row.id} className="hover:bg-gray-50 dark:hover:bg-gray-700">
            {columns.map(column => (
              <td
                key={column.key}
                className="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100"
              >
                {renderCell(row, column)}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  </div>
);
