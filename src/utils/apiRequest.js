const API_BASE_URL = 'http://192.168.1.209:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

// --- API Interaction Logic ---
export const apiRequest = async (endpoint, method = 'GET', body = null, responseType = 'json') => {
    const url = `${API_BASE_URL}${endpoint}`;
    const options = {
        method,
        headers: {
            // Only set Content-Type if there's a body
            ...(body && { 'Content-Type': 'application/json' }),
            // Add Authorization headers if implementing auth
        },
    };
    if (body) {
        options.body = JSON.stringify(body);
    }

    console.log(`API Request: ${method} ${url}`, body || ''); // Log request

    try {
        const response = await fetch(url, options);

        // Handle different response types
        let responseData;
        if (responseType === 'text') {
            responseData = await response.text();
        } else { // Default to json
            responseData = await response.json();
        }

        console.log(`API Response: ${response.status} ${url}`, responseType === 'json' ? responseData : '(text response)'); // Log response

        if (!response.ok) {
            // Try to get error message from JSON body, otherwise use status text
            const errorMsg = (responseType === 'json' && responseData?.error)
                             || (responseType === 'json' && responseData?.message)
                             || response.statusText
                             || `HTTP error! status: ${response.status}`;
            throw new Error(errorMsg);
        }
        return responseData; // Contains 'message' or actual data (JSON or text)
    } catch (error) {
        console.error(`API Error (${method} ${url}):`, error);
        // Rethrow the error so calling function can handle it
        throw error;
    }
};



// --- Action Handlers ---
export const handleApiAction = async (actionFn, successMessage, errorMessagePrefix, showNotification) => {
    showNotification('Processing...', 'info');
    try {
        const result = await actionFn();
        showNotification(result?.message || successMessage, 'success');
        // fetchData(false); // Refresh data in the background after successful action
        return true; // Indicate success
    } catch (error) {
        showNotification(`${errorMessagePrefix}: ${error.message || 'Unknown error'}`, 'error');
        return false; // Indicate failure
    }
};