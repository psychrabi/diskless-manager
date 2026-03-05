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
            <td colSpan="11" className="text-center py-4 text-base-content/60">
              No clients configured.
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
};

export default ClientTableEmptyState;
