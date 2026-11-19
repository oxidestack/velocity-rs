//! `org.apache.velocity.tools.XmlUtils` over a port of what it uses from the JDK: the DOM parser ([`dom`]),
//! XPath ([`xpath`]) and the identity transformer's serializer ([`serialize`]); plus the DOM nodes as values
//! templates can call (`XmlTool.node()`).

pub mod dom;
pub mod serialize;
pub mod xpath;

use std::sync::Arc;

use java_compat::lang::{JList, JObject, JValue, ListKind, MapKind, JMap};
use java_compat::reflect::{ClassDef, JType};

pub use dom::Node;

/// `String.compareTo`: by UTF-16 code units.
pub fn java_compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `XmlUtils.parse(xml)`: the document element, `None` (logged) when the XML is not well-formed.
pub fn parse(xml: &str) -> Option<Node> {
    match dom::parse(xml) {
        Ok(doc) => doc.document_element(),
        Err(e) => {
            crate::tools::log::error("org.apache.velocity.tools.XmlUtils", &format!("could not parse given xml: [Fatal Error] :{}:{}: {}", e.line, e.column, e.message));
            None
        }
    }
}

/// `XmlUtils.search(xpath, context)` / `getNodes`.
pub fn search(expression: &str, context: &Node) -> Result<Vec<Node>, xpath::XPathError> {
    xpath::select(expression, context)
}

/// `XmlUtils.getAttribute(node, attr, def)`.
pub fn get_attribute(node: &Node, attr: &str, default: Option<&str>) -> Option<String> {
    match node.attribute_node(attr) {
        Some(a) => a.node_value(),
        None => default.map(str::to_string),
    }
}

/// `XmlUtils.nodePath(n)`: `/a[1]/b[@id='x']/c[2]` (the buffer starts empty: `new StringBuffer('/')` sets
/// a capacity).
pub fn node_path(n: &Node) -> Option<String> {
    let mut hierarchy = vec![n.clone()];
    let mut parent = match n.node_type() {
        dom::ATTRIBUTE_NODE => n.owner_element(),
        dom::ELEMENT_NODE | dom::DOCUMENT_NODE => n.parent(),
        _ => return None,
    };
    while let Some(p) = parent {
        if p.node_type() == dom::DOCUMENT_NODE {
            break;
        }
        hierarchy.push(p.clone());
        parent = p.parent();
    }
    let mut buffer = String::new();
    while let Some(node) = hierarchy.pop() {
        if node.is_element() {
            if buffer.chars().count() == 1 {
                buffer.push_str(&node.node_name());
            } else {
                buffer.push('/');
                buffer.push_str(&node.node_name());
                let mut handled = false;
                if node.has_attributes() {
                    if node.has_attribute("id") {
                        buffer.push_str(&format!("[@id='{}']", node.get_attribute("id")));
                        handled = true;
                    } else if node.has_attribute("name") {
                        buffer.push_str(&format!("[@name='{}']", node.get_attribute("name")));
                        handled = true;
                    }
                }
                if !handled {
                    let mut prev = 1;
                    let mut s = node.previous_sibling();
                    while let Some(p) = s {
                        if p.node_type() == node.node_type() && p.node_name().to_lowercase() == node.node_name().to_lowercase() {
                            prev += 1;
                        }
                        s = p.previous_sibling();
                    }
                    buffer.push_str(&format!("[{prev}]"));
                }
            }
        } else if node.is_attr() {
            buffer.push_str("/@");
            buffer.push_str(&node.node_name());
        }
    }
    Some(buffer)
}

/// `XmlUtils.nodeToString(node)`.
pub fn node_to_string(n: &Node) -> String {
    serialize::node_to_string(n)
}

/// A DOM node as a template value.
pub struct NodeObject(pub Node);

impl NodeObject {
    /// The node as a template value.
    pub fn value(n: Node) -> JValue {
        JValue::Object(Arc::new(NodeObject(n)) as Arc<dyn JObject>)
    }

    fn java_string(&self) -> String {
        self.0.java_string()
    }
}

/// The node behind a DOM node value.
pub fn as_node(v: &JValue) -> Option<Node> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<NodeObject>().map(|n| n.0.clone()),
        _ => None,
    }
}

fn opt_node(n: Option<Node>) -> JValue {
    n.map(NodeObject::value).unwrap_or(JValue::Null)
}

fn opt_str(s: Option<String>) -> JValue {
    s.map(JValue::str).unwrap_or(JValue::Null)
}

impl JObject for NodeObject {
    fn class_name(&self) -> String {
        self.0.class_name().into()
    }

    fn invoke(&self, _name: &str, _args: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        self.java_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static ELEMENT: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::ELEMENT_NODE));
        static ATTR: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::ATTRIBUTE_NODE));
        static TEXT: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::TEXT_NODE));
        static CDATA: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::CDATA_SECTION_NODE));
        static COMMENT: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::COMMENT_NODE));
        static PI: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::PROCESSING_INSTRUCTION_NODE));
        static DOCTYPE: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::DOCUMENT_TYPE_NODE));
        static DOCUMENT: java_compat::reflect::LazyClass = java_compat::reflect::LazyClass::new(|| node_def(dom::DOCUMENT_NODE));
        Some(match self.0.node_type() {
            dom::ELEMENT_NODE => ELEMENT.get(),
            dom::ATTRIBUTE_NODE => ATTR.get(),
            dom::TEXT_NODE => TEXT.get(),
            dom::CDATA_SECTION_NODE => CDATA.get(),
            dom::COMMENT_NODE => COMMENT.get(),
            dom::PROCESSING_INSTRUCTION_NODE => PI.get(),
            dom::DOCUMENT_TYPE_NODE => DOCTYPE.get(),
            _ => DOCUMENT.get(),
        })
    }
}

fn node_list(nodes: Vec<Node>) -> JValue {
    JValue::List(JList::new(ListKind::Fixed, nodes.into_iter().map(NodeObject::value).collect()))
}

/// The read-only `org.w3c.dom.Node` API (and `Element`'s attribute getters) of a node kind.
fn node_def(kind: i32) -> Arc<ClassDef> {
    let probe = Node { doc: Arc::new(dom::Document { nodes: vec![dom::NodeData { kind, name: String::new(), value: String::new(), namespace_uri: None, prefix: None, local_name: None, parent: None, children: Vec::new(), attributes: Vec::new() }] }), id: 0 };
    let name = probe.class_name();
    let node = || JType::class("org.w3c.dom.Node");
    let s = JType::string;
    type N = NodeObject;
    let mut def = ClassDef::new(name)
        .implements(&["org.w3c.dom.Node"])
        .method::<N>("getNodeName", &[], Some(s()), |n, _, _| Ok(Some(JValue::str(n.0.node_name()))))
        .method::<N>("getNodeValue", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.node_value()))))
        .method::<N>("getNodeType", &[], Some(JType::Short), |n, _, _| Ok(Some(JValue::Short(n.0.node_type() as i16))))
        .method::<N>("getParentNode", &[], Some(node()), |n, _, _| Ok(Some(opt_node(n.0.parent()))))
        .method::<N>("getChildNodes", &[], Some(JType::class("org.w3c.dom.NodeList")), |n, _, _| Ok(Some(node_list(n.0.children()))))
        .method::<N>("getFirstChild", &[], Some(node()), |n, _, _| Ok(Some(opt_node(n.0.first_child()))))
        .method::<N>("getLastChild", &[], Some(node()), |n, _, _| Ok(Some(opt_node(n.0.last_child()))))
        .method::<N>("getPreviousSibling", &[], Some(node()), |n, _, _| Ok(Some(opt_node(n.0.previous_sibling()))))
        .method::<N>("getNextSibling", &[], Some(node()), |n, _, _| Ok(Some(opt_node(n.0.next_sibling()))))
        .method::<N>("getOwnerDocument", &[], Some(JType::class("org.w3c.dom.Document")), |n, _, _| Ok(Some(NodeObject::value(n.0.owner_document()))))
        .method::<N>("hasChildNodes", &[], Some(JType::Boolean), |n, _, _| Ok(Some(JValue::Bool(!n.0.children().is_empty()))))
        .method::<N>("getNamespaceURI", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.namespace_uri()))))
        .method::<N>("getPrefix", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.prefix()))))
        .method::<N>("getLocalName", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.local_name()))))
        .method::<N>("hasAttributes", &[], Some(JType::Boolean), |n, _, _| Ok(Some(JValue::Bool(n.0.has_attributes()))))
        .method::<N>("getTextContent", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.text_content()))))
        .method::<N>("toString", &[], Some(s()), |n, _, _| Ok(Some(JValue::str(n.java_string()))));
    if kind == dom::ELEMENT_NODE {
        def = def
            .method::<N>("getTagName", &[], Some(s()), |n, _, _| Ok(Some(JValue::str(n.0.node_name()))))
            .method::<N>("getAttribute", &[s()], Some(s()), |n, _, a| Ok(Some(JValue::str(n.0.get_attribute(a[0].as_str().unwrap_or("null"))))))
            .method::<N>("hasAttribute", &[s()], Some(JType::Boolean), |n, _, a| Ok(Some(JValue::Bool(n.0.has_attribute(a[0].as_str().unwrap_or("null"))))))
            .method::<N>("getAttributeNode", &[s()], Some(JType::class("org.w3c.dom.Attr")), |n, _, a| Ok(Some(opt_node(n.0.attribute_node(a[0].as_str().unwrap_or("null"))))))
            .method::<N>("getAttributes", &[], Some(JType::class("org.w3c.dom.NamedNodeMap")), |n, _, _| {
                let m = JMap::new(MapKind::Linked);
                for a in n.0.attributes() {
                    m.put_str(&a.node_name(), NodeObject::value(a));
                }
                Ok(Some(JValue::Map(m)))
            });
    }
    if kind == dom::ATTRIBUTE_NODE {
        def = def
            .method::<N>("getName", &[], Some(s()), |n, _, _| Ok(Some(JValue::str(n.0.node_name()))))
            .method::<N>("getValue", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.node_value()))))
            .method::<N>("getOwnerElement", &[], Some(JType::class("org.w3c.dom.Element")), |n, _, _| Ok(Some(opt_node(n.0.owner_element()))));
    }
    if matches!(kind, dom::TEXT_NODE | dom::CDATA_SECTION_NODE | dom::COMMENT_NODE) {
        def = def
            .method::<N>("getData", &[], Some(s()), |n, _, _| Ok(Some(opt_str(n.0.node_value()))))
            .method::<N>("getLength", &[], Some(JType::Int), |n, _, _| Ok(Some(JValue::Int(n.0.node_value().unwrap_or_default().encode_utf16().count() as i32))));
    }
    if kind == dom::DOCUMENT_NODE {
        def = def.method::<N>("getDocumentElement", &[], Some(JType::class("org.w3c.dom.Element")), |n, _, _| Ok(Some(opt_node(n.0.document_element()))));
    }
    def.register()
}
