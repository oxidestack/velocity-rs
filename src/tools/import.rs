//! `ImportSupport`, which `$json` and `$xml` inherit: `read(resource)` loads a readable file, else a class path
//! resource ([`super::classpath`]); `fetch(url)` loads a remote URL. Fetching goes through a process-wide hook
//! the embedding application installs; without one nothing is fetched, as when the connection fails.

use std::sync::{Arc, OnceLock};

use java_compat::lang::{JError, JObject, JValue};
use java_compat::reflect::{ClassDef, JType};

use super::{c, s, text, val};

/// Fetches a remote URL's body, decoded by its `Content-Type` charset; `Err` is logged and ignored by the tools.
pub type Fetcher = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

static FETCHER: OnceLock<Fetcher> = OnceLock::new();

/// Installs the URL fetcher (once per process).
pub fn set_fetcher(f: Fetcher) {
    let _ = FETCHER.set(f);
}

const VALID_SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+.-";

/// `ImportSupport.getProtocol`.
pub fn protocol(url: &str) -> Option<String> {
    let colon = url.find(':')?;
    url[..colon].chars().all(|ch| VALID_SCHEME_CHARS.contains(ch)).then(|| url[..colon].to_string())
}

/// `getResourceReader(resource)` read to the end: a readable regular file, else a class path resource found
/// for `caller` (the tool's class), as UTF-8.
pub fn resource_string(resource: &str, caller: &str) -> Option<String> {
    let path = std::path::Path::new(resource);
    if path.is_file() {
        if let Ok(b) = std::fs::read(path) {
            return Some(String::from_utf8_lossy(&b).into_owned());
        }
    }
    super::classpath::get_resource(resource, caller).map(|r| r.text())
}

/// `ImportSupport.RESOURCE_KEY`: the configuration key for a file or class path resource the tool reads when
/// configured.
pub const RESOURCE_KEY: &str = "resource";
/// `ImportSupport.URL_KEY`: the configuration key for a remote URL the tool fetches when configured.
pub const URL_KEY: &str = "url";
/// The class name of `org.apache.velocity.tools.generic.ImportSupport`.
pub const IMPORT_SUPPORT: &str = "org.apache.velocity.tools.generic.ImportSupport";

/// `acquireReader(url)` read to the end: only remote URLs (with a scheme) are supported.
pub fn fetch(url: &str) -> Option<String> {
    protocol(url)?;
    FETCHER.get().and_then(|f| f(url).ok())
}

fn io_error(message: &str) -> JError {
    JError::new("java.io.IOException", message)
}

/// `ImportSupport`'s public methods.
pub(crate) fn import_support_methods<T: JObject + 'static>(def: ClassDef) -> ClassDef {
    const IMPORT: &str = IMPORT_SUPPORT;
    def.method_of::<T>(IMPORT, "setSafeMode", &[JType::Boolean], None, |_, _, _| Ok(None))
        .method_of::<T>(IMPORT, "acquireString", &[s()], Some(s()), |_, _, a| {
            let url = a[0].as_str().unwrap_or("null");
            if protocol(url).is_none() {
                return Err(io_error("Only remote URLs are supported"));
            }
            match FETCHER.get().map(|f| f(url)) {
                Some(Ok(body)) => text(Some(body)),
                Some(Err(e)) => Err(io_error(&format!("Problem accessing the remote URL \"{url}\". {e}"))),
                None => Err(io_error(&format!("Problem accessing the remote URL \"{url}\". java.net.ConnectException: Connection refused"))),
            }
        })
        .method_of::<T>(IMPORT, "getResourceString", &[s()], Some(s()), |t, _, a| text(a[0].as_str().and_then(|r| resource_string(r, &t.class_name()))))
        .method_of::<T>(IMPORT, "acquireReader", &[s()], Some(c("java.io.Reader")), |_, _, a| {
            if a[0].as_str().and_then(protocol).is_none() {
                return Err(io_error("Only remote URLs are supported"));
            }
            // A `Reader` is not something a template can use; its class is all that renders.
            val(JValue::Null)
        })
        .method_of::<T>(IMPORT, "getResourceReader", &[s()], Some(c("java.io.Reader")), |_, _, _| val(JValue::Null))
}
