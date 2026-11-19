//! `org.apache.velocity.tools.generic.XmlTool` (`$xml`): a list of DOM nodes, navigated with XPath and
//! printed as XML.

use std::sync::{Arc, Weak};

use java_compat::lang::{JMap, JObject, JValue, MapKind};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::import::{self, RESOURCE_KEY, URL_KEY};
use super::safe::{self, SafeConfig, Tool};
use super::values::ValueParser;
use super::xml::{self, Node, NodeObject};
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.XmlTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.XmlTool";
const LOGGER: &str = "org.apache.velocity.tools.generic.XmlTool";

/// Port of `org.apache.velocity.tools.generic.XmlTool` (`$xml`): a list of DOM nodes navigated by attribute, index and
/// XPath.
pub struct XmlTool {
    safe: SafeConfig,
    nodes: Mutex<Option<Vec<Node>>>,
    me: Weak<XmlTool>,
}

impl XmlTool {
    /// `new XmlTool()`.
    pub fn new() -> Arc<XmlTool> {
        XmlTool::with(None)
    }

    /// `new XmlTool(List<Node>)`.
    pub fn of(nodes: Vec<Node>) -> Arc<XmlTool> {
        XmlTool::with(Some(nodes))
    }

    fn with(nodes: Option<Vec<Node>>) -> Arc<XmlTool> {
        Arc::new_cyclic(|me| XmlTool { safe: SafeConfig::default(), nodes: Mutex::new(nodes), me: me.clone() })
    }

    fn this(&self) -> Arc<XmlTool> {
        self.me.upgrade().expect("a live tool")
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        JValue::Object(self.this() as Arc<dyn JObject>)
    }

    /// `SafeConfig.configure(Map)`: reads the document from `resource` or `url`; nothing once locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    fn nodes(&self) -> Vec<Node> {
        self.nodes.lock().clone().unwrap_or_default()
    }

    /// `setRoot(node)`: a document stands for its document element.
    fn set_root(&self, node: Option<Node>) {
        *self.nodes.lock() = node.map(|n| {
            let n = if n.node_type() == xml::dom::DOCUMENT_NODE { n.document_element().unwrap_or(n) } else { n };
            vec![n]
        });
    }

    /// `parse(xml)`: `null` leaves the tool as it was; malformed XML empties it.
    pub fn parse(&self, source: Option<&str>) -> Arc<XmlTool> {
        if let Some(x) = source {
            self.set_root(xml::parse(x));
        }
        self.this()
    }

    /// `read(resource)`: a file or class path resource.
    pub fn read(&self, resource: Option<&str>) -> Arc<XmlTool> {
        match resource.and_then(|r| import::resource_string(r, CLASS)) {
            Some(content) => self.set_root(xml::parse(&content)),
            None => {
                if resource.is_none() {
                    super::log::error(LOGGER, "could not read XML resource null");
                }
            }
        }
        self.this()
    }

    /// `fetch(url)`: a remote URL through the installed fetcher.
    pub fn fetch(&self, url: Option<&str>) -> Arc<XmlTool> {
        if let Some(content) = url.and_then(import::fetch) {
            self.set_root(xml::parse(&content));
        }
        self.this()
    }

    /// `get(Object)`: an attribute value, the n-th node, or the nodes a (relative) path finds.
    pub fn get(&self, o: &JValue) -> JValue {
        if self.is_empty() || o.is_null() {
            return JValue::Null;
        }
        if let Some(attr) = self.attr(o).filter(|a| !a.is_empty()) {
            return JValue::str(attr);
        }
        if let Some(i) = super::conversion::to_number(o) {
            return self.get_index(Some(super::num::int_value(&i))).map(|t| t.value()).unwrap_or(JValue::Null);
        }
        let mut s = o.to_java_string();
        if s.is_empty() {
            return JValue::Null;
        }
        if !s.contains('/') {
            s = format!("{}/{s}", self.get_path().unwrap_or_else(|| "null".into()));
        }
        self.find_str(Some(&s)).map(|t| t.value()).unwrap_or(JValue::Null)
    }

    /// `getName()`: the `name` attribute, else the node name.
    pub fn get_name(&self) -> JValue {
        let name = self.get(&JValue::str("name"));
        if !name.is_null() && name.as_str() != Some("") {
            return name;
        }
        self.get_node_name().map(JValue::str).unwrap_or(JValue::Null)
    }

    /// `getNodeName()`: the first node's name, `None` when empty.
    pub fn get_node_name(&self) -> Option<String> {
        self.node().map(|n| n.node_name())
    }

    /// `getPath()`: the first node's XPath from the root (`XmlUtils.nodePath`, such as `/a[1]/b[@id='x']`); `None` when
    /// empty or not an element or attribute.
    pub fn get_path(&self) -> Option<String> {
        self.node().and_then(|n| xml::node_path(&n))
    }

    /// `attr(o)`: the first node's attribute, `null` when empty.
    pub fn attr(&self, o: &JValue) -> Option<String> {
        if o.is_null() {
            return None;
        }
        let key = o.to_java_string();
        let node = self.node()?;
        if node.is_element() {
            let v = node.get_attribute(&key);
            if !v.is_empty() {
                return Some(v);
            }
        }
        None
    }

    /// `attributes()`: the first node's attributes (a `HashMap`), `null` for non-elements.
    pub fn attributes(&self) -> Option<JMap> {
        let node = self.node()?;
        if !node.is_element() {
            return None;
        }
        let m = JMap::new(MapKind::Hash);
        for a in node.attributes() {
            m.put_str(&a.node_name(), JValue::str(a.node_value().unwrap_or_default()));
        }
        Some(m)
    }

    /// `isEmpty()`: whether the tool holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.lock().as_ref().is_none_or(|n| n.is_empty())
    }

    /// `size()`: how many nodes the tool holds.
    pub fn size(&self) -> usize {
        self.nodes.lock().as_ref().map(|n| n.len()).unwrap_or(0)
    }

    /// `iterator()`: a `NodeIterator` of single-node tools, `null` when empty.
    pub fn iterator(&self) -> Option<Vec<Arc<XmlTool>>> {
        if self.is_empty() {
            return None;
        }
        Some(self.nodes().into_iter().map(|n| XmlTool::of(vec![n])).collect())
    }

    /// `getFirst()`: a tool over the first node (this tool when it has just one); `None` when empty.
    pub fn get_first(&self) -> Option<Arc<XmlTool>> {
        match self.size() {
            1 => Some(self.this()),
            0 => None,
            _ => self.node().map(|n| XmlTool::of(vec![n])),
        }
    }

    /// `getLast()`: a tool over the last node (this tool when it has just one); `None` when empty.
    pub fn get_last(&self) -> Option<Arc<XmlTool>> {
        match self.size() {
            1 => Some(self.this()),
            0 => None,
            n => Some(XmlTool::of(vec![self.nodes()[n - 1].clone()])),
        }
    }

    /// `get(Number)`.
    pub fn get_index(&self, n: Option<i32>) -> Option<Arc<XmlTool>> {
        let i = n?;
        if self.is_empty() || i < 0 || i as usize > self.size() - 1 {
            return None;
        }
        Some(XmlTool::of(vec![self.nodes()[i as usize].clone()]))
    }

    /// `node()`: the first node.
    pub fn node(&self) -> Option<Node> {
        self.nodes.lock().as_ref().and_then(|n| n.first().cloned())
    }

    /// `find(Object)`.
    pub fn find(&self, o: &JValue) -> Option<Arc<XmlTool>> {
        if o.is_null() || self.is_empty() {
            return None;
        }
        self.find_str(Some(&o.to_java_string()))
    }

    /// `find(String xpath)`: a name alone searches descendants (`//name`); results of every node, in order.
    pub fn find_str(&self, xpath: Option<&str>) -> Option<Arc<XmlTool>> {
        let xpath = xpath.filter(|x| !x.is_empty())?;
        if self.is_empty() {
            return None;
        }
        let xpath = if xpath.contains('/') { xpath.to_string() } else { format!("//{xpath}") };
        let mut found = Vec::new();
        for n in self.nodes() {
            match xml::search(&xpath, &n) {
                Ok(list) => found.extend(list),
                Err(e) => {
                    super::log::error(LOGGER, &format!("could not parse XML expression '{xpath}': {}", e.0));
                    return None;
                }
            }
        }
        if found.is_empty() {
            return None;
        }
        Some(XmlTool::of(found))
    }

    /// `getParent()`: `null` at the document element.
    pub fn get_parent(&self) -> Option<Arc<XmlTool>> {
        let parent = self.node()?.parent()?;
        if parent.node_type() == xml::dom::DOCUMENT_NODE {
            return None;
        }
        Some(XmlTool::of(vec![parent]))
    }

    /// `parents()`: the distinct parents (owner elements for attributes).
    /// A document element among several nodes throws: its parent is cast to `Element`.
    pub fn parents(&self) -> Result<Option<Arc<XmlTool>>, java_compat::lang::JError> {
        if self.is_empty() {
            return Ok(None);
        }
        if self.size() == 1 {
            return Ok(self.get_parent());
        }
        let mut parents: Vec<Node> = Vec::new();
        for n in self.nodes() {
            let parent = if n.is_element() {
                match n.parent() {
                    Some(p) if !p.is_element() => {
                        return Err(java_compat::lang::JError::new(
                            "java.lang.ClassCastException",
                            "class com.sun.org.apache.xerces.internal.dom.DeferredDocumentImpl cannot be cast to class org.w3c.dom.Element (com.sun.org.apache.xerces.internal.dom.DeferredDocumentImpl and org.w3c.dom.Element are in module java.xml of loader 'bootstrap')",
                        ));
                    }
                    p => p,
                }
            } else if n.is_attr() {
                n.owner_element()
            } else {
                None
            };
            if let Some(p) = parent {
                if !parents.contains(&p) {
                    parents.push(p);
                }
            }
        }
        if parents.is_empty() {
            return Ok(None);
        }
        Ok(Some(XmlTool::of(parents)))
    }

    /// `children()`: every element's children but whitespace-only text.
    pub fn children(&self) -> Option<Arc<XmlTool>> {
        if self.is_empty() {
            return None;
        }
        let mut kids = Vec::new();
        for n in self.nodes() {
            if n.is_element() {
                for child in n.children() {
                    if (child.node_type() == xml::dom::TEXT_NODE || child.node_type() == xml::dom::CDATA_SECTION_NODE)
                        && java_trim(&child.node_value().unwrap_or_default()).is_empty()
                    {
                        continue;
                    }
                    kids.push(child);
                }
            }
        }
        Some(XmlTool::of(kids))
    }

    /// `getText()`: the trimmed text of every node, concatenated.
    pub fn get_text(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut out = String::new();
        for n in self.nodes() {
            let t = n.text_content().unwrap_or_else(|| "null".into());
            out.push_str(java_trim(&t));
        }
        let r = java_trim(&out);
        (!r.is_empty()).then(|| r.to_string())
    }

    fn java_string(&self) -> String {
        if self.is_empty() {
            return identity(self);
        }
        let mut out = String::new();
        for n in self.nodes() {
            if n.is_attr() {
                out.push_str(java_trim(&n.node_value().unwrap_or_default()));
            } else {
                out.push_str(&xml::node_to_string(&n));
            }
        }
        java_trim(&out).to_string()
    }
}

/// `String.trim()`: every character up to U+0020 at both ends.
pub(crate) fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

impl Tool for XmlTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }

    fn configure_values(&self, values: &ValueParser) {
        if let Some(resource) = values.get_string(RESOURCE_KEY) {
            self.read(Some(&resource));
        } else if let Some(url) = values.get_string(URL_KEY) {
            self.fetch(Some(&url));
        }
    }
}

fn opt_tool(t: Option<Arc<XmlTool>>) -> super::Ret {
    val(t.map(|t| t.value()).unwrap_or(JValue::Null))
}

/// The tool behind an `XmlTool` value.
pub fn as_xml_tool(v: &JValue) -> Option<Arc<XmlTool>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<XmlTool>().and_then(|t| t.me.upgrade()),
        _ => None,
    }
}

fn def() -> Arc<ClassDef> {
    type X = XmlTool;
    let me = || c(CLASS);
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<X>("parse", &[s()], Some(me()), |t, _, a| val(t.parse(a[0].as_str()).value()))
        .method::<X>("read", &[s()], Some(me()), |t, _, a| val(t.read(a[0].as_str()).value()))
        .method::<X>("fetch", &[s()], Some(me()), |t, _, a| val(t.fetch(a[0].as_str()).value()))
        .method::<X>("get", &[o()], Some(o()), |t, _, a| val(t.get(&a[0])))
        .method::<X>("getName", &[], Some(o()), |t, _, _| val(t.get_name()))
        .method::<X>("getNodeName", &[], Some(s()), |t, _, _| super::text(t.get_node_name()))
        .method::<X>("getPath", &[], Some(s()), |t, _, _| super::text(t.get_path()))
        .method::<X>("attr", &[o()], Some(s()), |t, _, a| super::text(t.attr(&a[0])))
        .method::<X>("attributes", &[], Some(c("java.util.Map")), |t, _, _| val(t.attributes().map(JValue::Map).unwrap_or(JValue::Null)))
        .method::<X>("isEmpty", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.is_empty())))
        .method::<X>("size", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.size() as i32)))
        .method::<X>("iterator", &[], Some(c("java.util.Iterator")), |t, _, _| {
            val(t.iterator().map(|v| super::iter::JIterator::value("org.apache.velocity.tools.generic.XmlTool$NodeIterator", v.into_iter().map(|t| t.value()).collect())).unwrap_or(JValue::Null))
        })
        .method::<X>("getFirst", &[], Some(me()), |t, _, _| opt_tool(t.get_first()))
        .method::<X>("getLast", &[], Some(me()), |t, _, _| opt_tool(t.get_last()))
        .method::<X>("get", &[c("java.lang.Number")], Some(me()), |t, _, a| opt_tool(if a[0].is_null() { None } else { t.get_index(Some(super::num::int_value(&a[0]))) }))
        .method::<X>("node", &[], Some(c("org.w3c.dom.Node")), |t, _, _| val(t.node().map(NodeObject::value).unwrap_or(JValue::Null)))
        .method::<X>("find", &[o()], Some(me()), |t, _, a| opt_tool(t.find(&a[0])))
        .method::<X>("find", &[s()], Some(me()), |t, _, a| opt_tool(t.find_str(a[0].as_str())))
        .method::<X>("getParent", &[], Some(me()), |t, _, _| opt_tool(t.get_parent()))
        .method::<X>("parents", &[], Some(me()), |t, _, _| opt_tool(t.parents()?))
        .method::<X>("children", &[], Some(me()), |t, _, _| opt_tool(t.children()))
        .method::<X>("getText", &[], Some(s()), |t, _, _| super::text(t.get_text()))
        .method::<X>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<X>(safe::safe_config_methods::<X>(def)).register()
}

tool_object!(XmlTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || XmlTool::new().value()).annotated(Annotation::default_key("xml"))
}
