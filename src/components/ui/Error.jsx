export const Error = ({ error }) => {
	return (
		<div className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded relative mb-6 dark:bg-red-900 dark:border-red-700 dark:text-red-200" role="alert" >
			<strong className="font-bold mr-2">Error:</strong>
			<span className="block sm:inline">{error}</span>
		</div>
	)
}