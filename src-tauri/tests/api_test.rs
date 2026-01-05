use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;

fn test_api_connection() {
    // Test if the API server is running on port 8080
    match TcpStream::connect("127.0.0.1:8080") {
        Ok(mut stream) => {
            println!("✓ Successfully connected to API server on port 8080");
            
            // Set a timeout for read operations
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            
            // Send a simple HTTP request to the root path
            let request = "GET /api/clients HTTP/1.1\r\nHost: 127.0.0.1:8080\r\n\r\n";
            stream.write_all(request.as_bytes()).unwrap();
            
            // Read the response
            let mut response = [0; 1024];
            let bytes_read = stream.read(&mut response).unwrap();
            
            let response_str = String::from_utf8_lossy(&response[..bytes_read]);
            println!("Response: {}", response_str);
        }
        Err(e) => {
            eprintln!("✗ Failed to connect to API server: {}", e);
        }
    }
}

fn main() {
    println!("Testing API endpoints...");
    test_api_connection();
}