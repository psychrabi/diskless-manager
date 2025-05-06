import { useNotification } from '../../contexts/NotificationContext';
import { X } from 'lucide-react';

const Notification = () => {
    const { notification, hideNotification } = useNotification();
 
   return notification.message && (    
        <div className={`px-4 py-3 rounded relative mb-6 border transition-opacity duration-300 ${
            notification.type === 'error' ? 'bg-red-100 border-red-400 text-red-700 dark:bg-red-900 dark:border-red-700 dark:text-red-200' :
            notification.type === 'success' ? 'bg-green-100 border-green-400 text-green-700 dark:bg-green-900 dark:border-green-700 dark:text-green-200' :
            'bg-blue-100 border-blue-400 text-blue-700 dark:bg-blue-900 dark:border-blue-700 dark:text-blue-200'
        }`} role="alert">
            <span className="block sm:inline">{notification.message}</span>
                <button onClick={() => hideNotification()} className="absolute top-0 bottom-0 right-0 px-4 py-3">
                <X className={`h-5 w-5 ${notification.type === 'error' ? 'text-red-500' : notification.type === 'success' ? 'text-green-500': 'text-blue-500'}`}/>
                </button>
        </div>
    );
};

export default Notification;


    
    