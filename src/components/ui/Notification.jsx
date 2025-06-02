import { useNotification } from '../../contexts/NotificationContext';
import { X } from 'lucide-react';

export const Notification = () => {
    const { notification, hideNotification } = useNotification();

    if (!notification.message) return null;

    const typeClass =
        notification.type === 'error'
            ? 'alert-error'
            : notification.type === 'success'
            ? 'alert-success'
            : 'alert-info';

    return (
        <div className={`alert ${typeClass} relative mb-6 transition-opacity duration-300`} role="alert">
            <span>{notification.message}</span>
            <button onClick={hideNotification} className="absolute top-0 bottom-0 right-0 px-4 py-3">
                <X className="h-5 w-5" />
            </button>
        </div>
    );
};


    
    