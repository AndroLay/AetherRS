//! Sumber: `tests/w12nl_json_test.cpp` (1-70). Status: lengkap; empat tes dengan literal JSON identik dengan sumber.
//! Tes murni: tanpa registry, file, atau proses. Jalankan dengan `cargo test --test nl_json`.

use w12::nl_json::{
    nl_json_parse_api_token, nl_json_parse_devices, nl_json_read_bool, NlJsonApiToken,
    NlJsonDeviceList,
};

/// Potong buffer ber-NUL di NUL pertama (setara strcmp dan strlen pada string C).
fn c_text(buf: &[u8]) -> &[u8] {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    &buf[..end]
}

#[test]
fn selects_online_android() {
    let json = concat!(
        "{\"ok\":true,\"devices\":[",
        "{\"name\":\"PC\",\"platform\":\"windows\",\"online\":true,\"link\":\"lan\",\"battery\":null},",
        "{\"name\":\"Pixel 9 Pro\",\"platform\":\"android\",\"online\":false,\"link\":\"offline\",\"battery\":{\"level\":49,\"charging\":false}},",
        "{\"name\":\"Pixel 9 Pro\",\"platform\":\"android\",\"online\":true,\"link\":\"relay\",\"battery\":{\"level\":49,\"charging\":true}}]}"
    );
    let mut result = NlJsonDeviceList::default();
    assert!(nl_json_parse_devices(json.as_bytes(), &mut result));
    assert_eq!(result.android_count, 2);
    assert!(result.has_android);
    assert_eq!(c_text(&result.primary.name), b"Pixel 9 Pro");
    assert!(result.primary.online);
    assert_eq!(c_text(&result.primary.link), b"relay");
    assert!(
        result.primary.has_battery && result.primary.battery_level == 49 && result.primary.charging
    );
}

#[test]
fn escaped_unicode_name() {
    let json = concat!(
        "{\"ok\":true,\"devices\":[{\"name\":\"Phone \\u00E9 \\uD83D\\uDCF1\",",
        "\"platform\":\"android\",\"online\":true,\"link\":\"lan\",\"battery\":null}]}"
    );
    let mut result = NlJsonDeviceList::default();
    let expected = b"Phone \xC3\xA9 \xF0\x9F\x93\xB1";
    assert!(nl_json_parse_devices(json.as_bytes(), &mut result));
    assert!(result.has_android);
    assert_eq!(c_text(&result.primary.name), expected);
    assert!(!result.primary.has_battery);
}

#[test]
fn empty_and_malformed_responses() {
    let empty = "{\"ok\":true,\"devices\":[{\"platform\":\"windows\",\"name\":\"PC\"}]}";
    let malformed = "{\"ok\":true,\"devices\":[{\"platform\":\"android\"}]} trailing";
    let mut result = NlJsonDeviceList::default();
    assert!(nl_json_parse_devices(empty.as_bytes(), &mut result));
    assert!(!result.has_android && result.android_count == 0);
    assert!(!nl_json_parse_devices(malformed.as_bytes(), &mut result));
}

#[test]
fn token_and_boolean_settings() {
    let token_json = "{\"port\":53320,\"token\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"}";
    let invalid_token = "{\"port\":53320,\"token\":\"short\"}";
    let settings = "{\"theme\":\"dark\",\"awayMode\":true,\"localApiEnabled\":true}";
    let mut token = NlJsonApiToken::default();
    let mut away = false;
    assert!(nl_json_parse_api_token(token_json.as_bytes(), &mut token));
    assert_eq!(token.port, 53320);
    assert_eq!(c_text(&token.token).len(), 64);
    assert!(!nl_json_parse_api_token(invalid_token.as_bytes(), &mut token));
    assert!(nl_json_read_bool(settings.as_bytes(), "awayMode", Some(&mut away)));
    assert!(away);
}
