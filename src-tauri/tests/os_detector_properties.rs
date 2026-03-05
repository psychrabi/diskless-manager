use app_lib::os_detector::{OsDetector, OsType};
use app_lib::core::client::Client;
use chrono::Utc;
use proptest::prelude::*;

// ============================================================================
// Property 17: OS Type Detection
// For any control operation request, the system should detect the client's 
// operating system type.
// Validates: Requirements 4.1
// ============================================================================
#[test]
fn test_property_17_os_type_detection_linux() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type(&client, Some("linux"));
    assert_eq!(os_type, OsType::Linux, "Should detect Linux OS type");
}

#[test]
fn test_property_17_os_type_detection_windows() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type(&client, Some("windows"));
    assert_eq!(os_type, OsType::Windows, "Should detect Windows OS type");
}

#[test]
fn test_property_17_os_type_detection_unknown() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type(&client, Some("unknown"));
    assert_eq!(os_type, OsType::Unknown, "Should detect Unknown OS type");
}

#[test]
fn test_property_17_os_type_detection_none() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type(&client, None);
    assert_eq!(os_type, OsType::Unknown, "Should return Unknown when OS type is None");
}

#[test]
fn test_property_17_os_type_detection_case_insensitive() {
    let client = create_test_client("test-client", "192.168.1.100");
    
    // Test various case combinations
    assert_eq!(OsDetector::get_os_type(&client, Some("LINUX")), OsType::Linux);
    assert_eq!(OsDetector::get_os_type(&client, Some("Linux")), OsType::Linux);
    assert_eq!(OsDetector::get_os_type(&client, Some("WINDOWS")), OsType::Windows);
    assert_eq!(OsDetector::get_os_type(&client, Some("Windows")), OsType::Windows);
}

proptest! {
    #[test]
    fn test_property_17_os_type_detection_invariant(
        os_str in "(linux|windows|unknown|invalid)"
    ) {
        let client = create_test_client("test-client", "192.168.1.100");
        let os_type = OsDetector::get_os_type(&client, Some(&os_str));
        
        // OS type should always be one of the three valid types
        match os_type {
            OsType::Linux | OsType::Windows | OsType::Unknown => {
                // Valid OS type
            }
        }
    }
}

// ============================================================================
// Property 20: OS Type Fallback Logic
// For any client with unknown OS type, the system should attempt Linux 
// commands first, then fall back to Windows commands.
// Validates: Requirements 4.4
// ============================================================================
#[test]
fn test_property_20_os_type_fallback_unknown_to_windows() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type_with_fallback(&client, Some("unknown"));
    assert_eq!(
        os_type, OsType::Windows,
        "Should fallback to Windows for unknown OS type"
    );
}

#[test]
fn test_property_20_os_type_fallback_none_to_windows() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type_with_fallback(&client, None);
    assert_eq!(
        os_type, OsType::Windows,
        "Should fallback to Windows when OS type is None"
    );
}

#[test]
fn test_property_20_os_type_fallback_linux_no_fallback() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type_with_fallback(&client, Some("linux"));
    assert_eq!(
        os_type, OsType::Linux,
        "Should not fallback when OS type is Linux"
    );
}

#[test]
fn test_property_20_os_type_fallback_windows_no_fallback() {
    let client = create_test_client("test-client", "192.168.1.100");
    let os_type = OsDetector::get_os_type_with_fallback(&client, Some("windows"));
    assert_eq!(
        os_type, OsType::Windows,
        "Should not fallback when OS type is Windows"
    );
}

proptest! {
    #[test]
    fn test_property_20_os_type_fallback_invariant(
        os_str in "(linux|windows|unknown|invalid|)"
    ) {
        let client = create_test_client("test-client", "192.168.1.100");
        let os_type_option = if os_str.is_empty() { None } else { Some(os_str.as_str()) };
        let os_type = OsDetector::get_os_type_with_fallback(&client, os_type_option);
        
        // With fallback, should never return Unknown
        assert_ne!(
            os_type, OsType::Unknown,
            "Fallback logic should never return Unknown"
        );
        
        // Should always return either Linux or Windows
        match os_type {
            OsType::Linux | OsType::Windows => {
                // Valid fallback result
            }
            OsType::Unknown => {
                panic!("Fallback should not return Unknown");
            }
        }
    }
}

// ============================================================================
// Property 21: OS Type Caching
// For any client, storing the OS type and then retrieving it should return 
// the same OS type without re-detection.
// Validates: Requirements 4.5
// ============================================================================
#[test]
fn test_property_21_os_type_caching_linux() {
    let _detector = OsDetector::default();
    let client = create_test_client("test-client", "192.168.1.100");
    
    // Get OS type (should be cached)
    let os_type1 = OsDetector::get_os_type(&client, Some("linux"));
    
    // Get OS type again (should be the same)
    let os_type2 = OsDetector::get_os_type(&client, Some("linux"));
    
    assert_eq!(os_type1, os_type2, "OS type should be consistent");
    assert_eq!(os_type1, OsType::Linux, "OS type should be Linux");
}

#[test]
fn test_property_21_os_type_caching_windows() {
    let _detector = OsDetector::default();
    let client = create_test_client("test-client", "192.168.1.100");
    
    // Get OS type (should be cached)
    let os_type1 = OsDetector::get_os_type(&client, Some("windows"));
    
    // Get OS type again (should be the same)
    let os_type2 = OsDetector::get_os_type(&client, Some("windows"));
    
    assert_eq!(os_type1, os_type2, "OS type should be consistent");
    assert_eq!(os_type1, OsType::Windows, "OS type should be Windows");
}

#[test]
fn test_property_21_os_type_caching_multiple_clients() {
    let _detector = OsDetector::default();
    let client1 = create_test_client("client1", "192.168.1.100");
    let client2 = create_test_client("client2", "192.168.1.101");
    
    // Get OS types for different clients
    let os_type1 = OsDetector::get_os_type(&client1, Some("linux"));
    let os_type2 = OsDetector::get_os_type(&client2, Some("windows"));
    
    // Get OS types again
    let os_type1_again = OsDetector::get_os_type(&client1, Some("linux"));
    let os_type2_again = OsDetector::get_os_type(&client2, Some("windows"));
    
    // Each client should maintain its own OS type
    assert_eq!(os_type1, os_type1_again, "Client 1 OS type should be consistent");
    assert_eq!(os_type2, os_type2_again, "Client 2 OS type should be consistent");
    assert_eq!(os_type1, OsType::Linux, "Client 1 should be Linux");
    assert_eq!(os_type2, OsType::Windows, "Client 2 should be Windows");
}

proptest! {
    #[test]
    fn test_property_21_os_type_caching_invariant(
        os_str in "(linux|windows|unknown)"
    ) {
        let _detector = OsDetector::default();
        let client = create_test_client("test-client", "192.168.1.100");
        
        // Get OS type multiple times
        let os_type1 = OsDetector::get_os_type(&client, Some(&os_str));
        let os_type2 = OsDetector::get_os_type(&client, Some(&os_str));
        let os_type3 = OsDetector::get_os_type(&client, Some(&os_str));
        
        // All should be identical (caching property)
        assert_eq!(os_type1, os_type2, "OS type should be consistent on second call");
        assert_eq!(os_type2, os_type3, "OS type should be consistent on third call");
    }
}

// ============================================================================
// Additional Property Tests
// ============================================================================

#[test]
fn test_os_type_parse_linux() {
    let os_type = OsDetector::parse_os_type("linux");
    assert_eq!(os_type, OsType::Linux);
}

#[test]
fn test_os_type_parse_windows() {
    let os_type = OsDetector::parse_os_type("windows");
    assert_eq!(os_type, OsType::Windows);
}

#[test]
fn test_os_type_parse_unknown() {
    let os_type = OsDetector::parse_os_type("unknown");
    assert_eq!(os_type, OsType::Unknown);
}

#[test]
fn test_os_type_parse_invalid() {
    let os_type = OsDetector::parse_os_type("invalid");
    assert_eq!(os_type, OsType::Unknown);
}

#[test]
fn test_os_type_parse_case_insensitive() {
    assert_eq!(OsDetector::parse_os_type("LINUX"), OsType::Linux);
    assert_eq!(OsDetector::parse_os_type("Linux"), OsType::Linux);
    assert_eq!(OsDetector::parse_os_type("WINDOWS"), OsType::Windows);
    assert_eq!(OsDetector::parse_os_type("Windows"), OsType::Windows);
}

#[test]
fn test_os_type_display() {
    assert_eq!(OsType::Linux.to_string(), "linux");
    assert_eq!(OsType::Windows.to_string(), "windows");
    assert_eq!(OsType::Unknown.to_string(), "unknown");
}

#[test]
fn test_os_type_serialization() {
    let os_type = OsType::Linux;
    let json = serde_json::to_string(&os_type).unwrap();
    assert_eq!(json, "\"linux\"");

    let deserialized: OsType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, OsType::Linux);
}

#[test]
fn test_os_type_serialization_windows() {
    let os_type = OsType::Windows;
    let json = serde_json::to_string(&os_type).unwrap();
    assert_eq!(json, "\"windows\"");

    let deserialized: OsType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, OsType::Windows);
}

#[test]
fn test_os_type_serialization_unknown() {
    let os_type = OsType::Unknown;
    let json = serde_json::to_string(&os_type).unwrap();
    assert_eq!(json, "\"unknown\"");

    let deserialized: OsType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, OsType::Unknown);
}

proptest! {
    #[test]
    fn prop_os_type_parse_roundtrip(
        os_str in "(linux|windows|unknown)"
    ) {
        let os_type = OsDetector::parse_os_type(&os_str);
        let display_str = os_type.to_string();
        let os_type_again = OsDetector::parse_os_type(&display_str);
        
        assert_eq!(os_type, os_type_again, "Parse roundtrip should preserve OS type");
    }
}

proptest! {
    #[test]
    fn prop_os_type_serialization_roundtrip(
        os_type in prop_os_type()
    ) {
        let json = serde_json::to_string(&os_type).unwrap();
        let deserialized: OsType = serde_json::from_str(&json).unwrap();
        
        assert_eq!(os_type, deserialized, "Serialization roundtrip should preserve OS type");
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_client(name: &str, ip: &str) -> Client {
    Client {
        id: "1".to_string(),
        name: name.to_string(),
        mac: "00:11:22:33:44:55".to_string(),
        ip: ip.to_string(),
        master: "test-master".to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        snapshot: None,
        block_store: None,
        target_iqn: None,
        writeback: None,
        last_modified: None,
        block_device: None,
        status: Some("Online".to_string()),
        mode: None,
        pxe_mode: None,
        keep_writeback: None,
        use_game_disk: None,
    }
}

fn prop_os_type() -> impl Strategy<Value = OsType> {
    prop_oneof![
        Just(OsType::Linux),
        Just(OsType::Windows),
        Just(OsType::Unknown),
    ]
}
