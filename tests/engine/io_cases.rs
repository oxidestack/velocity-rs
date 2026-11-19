//! `UnicodeInputStreamTestCase`, `SimplePoolTestCase`, `RuntimeInstanceTest`.

use std::io::Read;
use std::sync::Arc;

use parking_lot::Mutex;
use velocity::io::{SimplePool, UTF8_BOM, UTF16BE_BOM, UTF16LE_BOM, UTF32BE_BOM, UTF32LE_BOM, UnicodeBom, UnicodeInputStream};
use velocity::props::{ExtProperties, PropValue};
use velocity::resource::{ResourceManagerApi, ResourceType};
use velocity::{Engine, Loaded, VelocityError};

/// `String.getBytes(enc)` for the encodings the test uses.
fn bytes(s: &str, enc: &str) -> Vec<u8> {
    match enc {
        "UTF-16BE" => s.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        "UTF-16LE" => s.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        _ => s.as_bytes().to_vec(),
    }
}

fn test_run(bom: Option<UnicodeBom>, s: Option<&str>, enc: &str, skip_bom: bool) {
    let str_bytes = s.map(|s| bytes(s, enc)).unwrap_or_default();
    let mut input = bom.map(|b| b.bytes.to_vec()).unwrap_or_default();
    input.extend(&str_bytes);
    let expected = match bom {
        Some(b) if !skip_bom => [b.bytes.to_vec(), str_bytes].concat(),
        _ => str_bytes,
    };
    let mut uis = UnicodeInputStream::new(input.as_slice(), skip_bom).unwrap();
    assert_eq!(uis.is_skip_bom(), skip_bom, "BOM Skipping problem");
    if bom.is_some() {
        assert_eq!(uis.encoding_from_stream(), Some(enc), "Wrong Encoding detected");
    }
    let mut result = Vec::new();
    uis.read_to_end(&mut result).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_simple_stream() {
    test_run(None, Some("Ich bin zwei Oeltanks"), "US-ASCII", true);
    test_run(None, Some("Ich bin zwei Oeltanks"), "US-ASCII", false);
}

#[test]
fn test_simple_utf8() {
    test_run(None, Some("Ich bin zwei Oeltanks"), "UTF-8", true);
    test_run(None, Some("Ich bin zwei Oeltanks"), "UTF-8", false);
}

#[test]
fn test_real_utf8() {
    test_run(None, Some("Ich bin zwei \u{00d6}ltanks"), "UTF-8", true);
    test_run(None, Some("Ich bin zwei \u{00d6}ltanks"), "UTF-8", false);
}

#[test]
fn test_real_utf8_with_bom() {
    test_run(Some(UTF8_BOM), Some("Ich bin ein Test"), "UTF-8", true);
    test_run(Some(UTF8_BOM), Some("Ich bin ein Test"), "UTF-8", false);
}

#[test]
fn test_real_utf16_be_with_bom() {
    test_run(Some(UTF16BE_BOM), Some("Ich bin ein Test"), "UTF-16BE", true);
    test_run(Some(UTF16BE_BOM), Some("Ich bin ein Test"), "UTF-16BE", false);
}

#[test]
fn test_real_utf16_le_with_bom() {
    test_run(Some(UTF16LE_BOM), Some("Ich bin ein Test"), "UTF-16LE", true);
    test_run(Some(UTF16LE_BOM), Some("Ich bin ein Test"), "UTF-16LE", false);
}

#[test]
fn test_real_utf32_be_with_bom() {
    test_run(Some(UTF32BE_BOM), None, "UTF-32BE", true);
    test_run(Some(UTF32BE_BOM), None, "UTF-32BE", false);
}

#[test]
fn test_real_utf32_le_with_bom() {
    test_run(Some(UTF32LE_BOM), None, "UTF-32LE", true);
    test_run(Some(UTF32LE_BOM), None, "UTF-32LE", false);
}

#[test]
fn test_pool() {
    let mut sp = SimplePool::new(10);
    for i in 0..10 {
        sp.put(i);
    }
    for i in (0..10).rev() {
        assert_eq!(sp.get(), Some(i));
    }
    assert!(sp.pool().iter().all(Option::is_none), "Pool not empty");
}

/// `RuntimeInstanceTest.MockResourceManager`: remembers the encoding it was asked for.
#[derive(Default)]
struct MockResourceManager {
    encoding: Mutex<Option<String>>,
}

impl ResourceManagerApi for MockResourceManager {
    fn get_resource(&self, _: &str, _: ResourceType, encoding: &str) -> Result<Option<Arc<Loaded>>, VelocityError> {
        *self.encoding.lock() = Some(encoding.to_string());
        Ok(None)
    }

    fn loader_name_for_resource(&self, _: &str) -> Option<String> {
        None
    }
}

#[test]
fn given_overriden_input_encoding_when_initializing_default_encoding_is_overridden() {
    let manager = Arc::new(MockResourceManager::default());
    let mut p = ExtProperties::new();
    p.add_property("resource.default_encoding", "testDummyEncoding".into());
    let instance: Arc<dyn ResourceManagerApi> = manager.clone();
    p.add_property("resource.manager.instance", PropValue::Object(Arc::new(instance)));
    let engine = Engine::with_properties(&p).unwrap();
    let _ = engine.get_template("some template", None);
    assert_eq!(manager.encoding.lock().as_deref(), Some("testDummyEncoding"));
}
