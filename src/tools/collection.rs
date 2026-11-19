//! `org.apache.velocity.tools.generic.CollectionTool` (`$collection`): `split` and the `sort` family; and the
//! deprecated `SortTool` (`$sorter`), which is the same tool under another name.

use std::sync::{Arc, Weak};

use java_compat::lang::{JArray, JError, JList, JMap, JObject, JValue, compare_ignore_case, compare_to};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::bean::get_property;
use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::{Ret, c, identity, o, object_methods, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.CollectionTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.CollectionTool";
/// The class name of `org.apache.velocity.tools.generic.SortTool`.
pub const SORT_TOOL: &str = "org.apache.velocity.tools.generic.SortTool";
/// `CollectionTool.STRINGS_DELIMITER_FORMAT_KEY`: the configuration key for the delimiter `split` uses.
pub const STRINGS_DELIMITER_FORMAT_KEY: &str = "stringsDelimiter";
/// `CollectionTool.STRINGS_TRIM_KEY`: the configuration key for trimming `split`'s parts.
pub const STRINGS_TRIM_KEY: &str = "trimStrings";
/// `CollectionTool.DEFAULT_STRINGS_DELIMITER`: `,`.
pub const DEFAULT_STRINGS_DELIMITER: &str = ",";

/// `CollectionTool`'s state, which `SortTool` inherits.
pub struct CollectionCore {
    safe: SafeConfig,
    st: Mutex<(String, bool)>,
}

impl Default for CollectionCore {
    fn default() -> CollectionCore {
        CollectionCore { safe: SafeConfig::default(), st: Mutex::new((DEFAULT_STRINGS_DELIMITER.into(), true)) }
    }
}

impl CollectionCore {
    /// `getStringsDelimiter()`: the regular expression `split` uses, `,` unless configured.
    pub fn get_strings_delimiter(&self) -> String {
        self.st.lock().0.clone()
    }

    /// `getStringsTrim()`: whether `split` trims its parts, `true` unless configured.
    pub fn get_strings_trim(&self) -> bool {
        self.st.lock().1
    }

    /// `setStringsDelimiter(d)`.
    pub fn set_strings_delimiter(&self, d: &str) {
        self.st.lock().0 = d.to_string();
    }

    /// `setStringsTrim(t)`.
    pub fn set_strings_trim(&self, t: bool) {
        self.st.lock().1 = t;
    }

    fn configure_values(&self, values: &ValueParser) {
        if let Some(d) = values.get_string(STRINGS_DELIMITER_FORMAT_KEY) {
            self.set_strings_delimiter(&d);
        }
        if let Some(t) = values.get_boolean(STRINGS_TRIM_KEY) {
            self.set_strings_trim(t);
        }
    }

    /// `split(value)`: around the delimiter (a regular expression), trimmed unless configured otherwise.
    pub fn split(&self, value: Option<&str>) -> Result<Option<Vec<String>>, JError> {
        let Some(v) = value else { return Ok(None) };
        let (delim, trim) = self.st.lock().clone();
        let parts = if !v.contains(&delim) { vec![v.to_string()] } else { java_compat::methods::split(v, &delim, 0)? };
        Ok(Some(if trim { parts.iter().map(|p| p.trim_matches(|c: char| c <= ' ').to_string()).collect() } else { parts }))
    }

    /// `sort(collection)` and friends (`null` for what cannot be sorted or compared).
    pub fn sort(&self, target: &JValue) -> Option<JValue> {
        sort(target, None).ok().flatten().filter(|v| !v.is_null())
    }

    /// `sort(collection, properties)`: by the named properties (each optionally suffixed `:asc`/`:desc`); `None` for
    /// what cannot be sorted or compared.
    pub fn sort_by(&self, target: &JValue, properties: &[JValue]) -> Option<JValue> {
        sort(target, Some(properties)).ok().flatten().filter(|v| !v.is_null())
    }
}

/// Port of `org.apache.velocity.tools.generic.CollectionTool` (`$collection`): splitting strings and sorting
/// collections, arrays and maps.
pub struct CollectionTool {
    core: CollectionCore,
    me: Weak<CollectionTool>,
}

impl std::ops::Deref for CollectionTool {
    type Target = CollectionCore;
    fn deref(&self) -> &CollectionCore {
        &self.core
    }
}

impl CollectionTool {
    /// `new CollectionTool()`: delimiter `,`, trimming.
    pub fn new() -> Arc<CollectionTool> {
        Arc::new_cyclic(|me| CollectionTool { core: CollectionCore::default(), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads the delimiter and trimming; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for CollectionTool {
    fn safe(&self) -> &SafeConfig {
        &self.core.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.core.configure_values(values);
    }
}

/// `SortTool` (deprecated): a `CollectionTool` under the key `sorter`.
pub struct SortTool {
    core: CollectionCore,
    me: Weak<SortTool>,
}

impl std::ops::Deref for SortTool {
    type Target = CollectionCore;
    fn deref(&self) -> &CollectionCore {
        &self.core
    }
}

impl SortTool {
    /// `new SortTool()`: delimiter `,`, trimming.
    pub fn new() -> Arc<SortTool> {
        Arc::new_cyclic(|me| SortTool { core: CollectionCore::default(), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: reads the delimiter and trimming; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

impl Tool for SortTool {
    fn safe(&self) -> &SafeConfig {
        &self.core.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        self.core.configure_values(values);
    }
}

/// The elements `sort` sorts: a collection's, an `Object[]`'s, or a map's values; `None` for other types.
fn elements(v: &JValue) -> Option<Vec<JValue>> {
    match v {
        JValue::List(l) => Some(l.items()),
        JValue::Map(m) => Some(m.entries().into_iter().map(|(_, v)| v).collect()),
        JValue::Array(a) if !a.component.is_primitive() => Some(a.items()),
        JValue::Object(o) if java_compat::reflect::class_def(v).is_some_and(|d| d.interfaces.iter().any(|i| &**i == "java.util.Collection")) => o.iterate(),
        _ => None,
    }
}

/// A stable sort (`Collections.sort` is a stable merge sort) that stops at the first failed comparison.
fn sort_by(mut items: Vec<JValue>, cmp: impl Fn(&JValue, &JValue) -> Result<i32, JError>) -> Result<Vec<JValue>, JError> {
    let mut error = None;
    items.sort_by(|a, b| {
        if error.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match cmp(a, b) {
            Ok(r) => r.cmp(&0),
            Err(e) => {
                error = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    match error {
        Some(e) => Err(e),
        None => Ok(items),
    }
}

/// `PropertiesComparator`: each property with an optional `:asc` / `:desc` suffix.
fn properties_compare(props: &[(String, i32)], lhs: &JValue, rhs: &JValue) -> Result<i32, JError> {
    let comparable = |o: &JValue, p: &str| {
        get_property(o, p).map_err(|e| {
            JError::new("java.lang.IllegalArgumentException", format!("Could not retrieve comparable value for '{p}' from {}: {e}", o.to_java_string()))
        })
    };
    for (p, dir) in props {
        let (left, right) = (comparable(lhs, p)?, comparable(rhs, p)?);
        let comparison = match (&left, &right) {
            (JValue::Null, JValue::Null) => 0,
            (JValue::Null, r) => -compare_to(r, &JValue::Null)?,
            (JValue::Str(l), JValue::Str(r)) => compare_ignore_case(l, r),
            (JValue::Str(_), _) => return Err(JError::new("java.lang.ClassCastException", format!("class {} cannot be cast to class java.lang.String", right.class_name()))),
            (l, r) => compare_to(l, r)?,
        };
        if comparison != 0 {
            return Ok(comparison * dir);
        }
    }
    Ok(0)
}

fn parse_properties(props: &[JValue]) -> Result<Vec<(String, i32)>, JError> {
    props
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.is_null() {
                return Err(JError::new("java.lang.IllegalArgumentException", format!("Property {i}is null, sort properties may not be null.")));
            }
            let p = p.to_java_string();
            Ok(match p.split_once(':') {
                Some((name, kind)) => (name.to_string(), if kind.eq_ignore_ascii_case("desc") { -1 } else { 1 }),
                None => (p, 1),
            })
        })
        .collect()
}

/// `sort(collection, properties)` → `internalSort`: `null` when a comparison throws.
fn internal_sort(items: Vec<JValue>, properties: Option<&[JValue]>) -> Ret {
    let items: Vec<JValue> = items.into_iter().filter(|v| !v.is_null()).collect();
    let sorted = match properties {
        None => sort_by(items, compare_to),
        Some(props) => match parse_properties(props) {
            Ok(props) => sort_by(items, |a, b| properties_compare(&props, a, b)),
            // The comparator's constructor throws out of `internalSort`'s try block too.
            Err(e) => Err(e),
        },
    };
    match sorted {
        Ok(v) => val(JValue::List(JList::array_list(v))),
        Err(_) => val(JValue::Null),
    }
}

fn sort(target: &JValue, properties: Option<&[JValue]>) -> Ret {
    match elements(target) {
        Some(items) => internal_sort(items, properties),
        None => val(JValue::Null),
    }
}

/// `sort(x, comparator)`: a Java `Comparator` object, called through its `compare` method.
fn sort_with(target: &JValue, comparator: &JValue) -> Ret {
    let Some(items) = elements(target) else { return val(JValue::Null) };
    let items: Vec<JValue> = items.into_iter().filter(|v| !v.is_null()).collect();
    let sorted = if comparator.is_null() {
        sort_by(items, compare_to)
    } else {
        sort_by(items, |a, b| match crate::introspect::call(comparator, "compare", &[a.clone(), b.clone()]) {
            Some(Ok(Some(JValue::Int(i)))) => Ok(i),
            Some(Err(e)) => Err(e),
            _ => Err(JError::without_message("java.lang.ClassCastException")),
        })
    };
    // `Collections.sort(list, comparator)` is outside any try block: its exceptions escape.
    let sorted = sorted?;
    if matches!(target, JValue::Array(_)) {
        return val(JArray::objects(sorted));
    }
    val(JValue::List(JList::array_list(sorted)))
}

fn list_arg(v: &JValue) -> Option<Vec<JValue>> {
    match v {
        JValue::Null => None,
        other => other.iterate(),
    }
}

fn methods<T: std::ops::Deref<Target = CollectionCore> + Tool + JObject>(def: ClassDef) -> ClassDef {
    let collection = || c("java.util.Collection");
    let map = || c("java.util.Map");
    let array = || JType::array(o());
    let comparator = || c("java.util.Comparator");
    let list = || c("java.util.List");
    let d = CLASS;
    let def = def
        .method_of::<T>(d, "split", &[s()], Some(JType::array(s())), |t, _, a| val(t.split(a[0].as_str())?.map(JArray::strings).unwrap_or(JValue::Null)))
        .method_of::<T>(d, "sort", &[collection(), comparator()], Some(collection()), |_, _, a| sort_with(&a[0], &a[1]))
        .method_of::<T>(d, "sort", &[array(), comparator()], Some(array()), |_, _, a| sort_with(&a[0], &a[1]))
        .method_of::<T>(d, "sort", &[map(), comparator()], Some(collection()), |_, _, a| sort_with(&a[0], &a[1]))
        .method_of::<T>(d, "sort", &[o(), comparator()], Some(collection()), |_, _, a| sort_with(&a[0], &a[1]))
        .method_of::<T>(d, "sort", &[collection()], Some(collection()), |_, _, a| sort(&a[0], None))
        .method_of::<T>(d, "sort", &[array()], Some(collection()), |_, _, a| sort(&a[0], None))
        .method_of::<T>(d, "sort", &[map()], Some(collection()), |_, _, a| sort(&a[0], None))
        .method_of::<T>(d, "sort", &[o()], Some(collection()), |_, _, a| sort(&a[0], None))
        .method_of::<T>(d, "sort", &[o(), s()], Some(collection()), |_, _, a| sort(&a[0], Some(&[a[1].clone()])))
        .method_of::<T>(d, "sort", &[collection(), list()], Some(collection()), |_, _, a| sort(&a[0], list_arg(&a[1]).as_deref()))
        .method_of::<T>(d, "sort", &[map(), list()], Some(collection()), |_, _, a| sort(&a[0], list_arg(&a[1]).as_deref()))
        .method_of::<T>(d, "sort", &[array(), list()], Some(collection()), |_, _, a| sort(&a[0], list_arg(&a[1]).as_deref()))
        .method_of::<T>(d, "getStringsDelimiter", &[], Some(s()), |t, _, _| val(JValue::str(t.get_strings_delimiter())))
        .method_of::<T>(d, "getStringsTrim", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.get_strings_trim())));
    object_methods::<T>(safe::safe_config_methods::<T>(def))
}

fn def() -> Arc<ClassDef> {
    methods::<CollectionTool>(ClassDef::new(CLASS).extends(&[safe::SAFE_CONFIG]).implements(&["java.io.Serializable"])).register()
}

tool_object!(CollectionTool, def);

fn sort_tool_def() -> Arc<ClassDef> {
    methods::<SortTool>(ClassDef::new(SORT_TOOL).extends(&[CLASS, safe::SAFE_CONFIG]).implements(&["java.io.Serializable"])).register()
}

tool_object!(SortTool, sort_tool_def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || CollectionTool::new().value()).annotated(Annotation::default_key("collection"))
}

pub(crate) fn sort_tool_class() -> JavaClass {
    JavaClass::tool(sort_tool_def(), || SortTool::new().value()).extends(Some(CLASS)).annotated(Annotation::default_key("sorter")).annotated(Annotation::deprecated())
}
