//! `XmlUtils.nodeToString`: the JDK's identity `Transformer` (Xalan's `ToXMLStream`) with the XML declaration
//! omitted, no indentation, UTF-8: empty elements as `<a/>`, attributes in DOM order with double quotes,
//! namespace declarations added where a name's namespace is not in scope in the output.

use super::dom::*;

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

struct Writer {
    out: String,
    /// Output namespace scopes: (prefix, uri) pairs, innermost last.
    scopes: Vec<Vec<(String, String)>>,
}

impl Writer {
    fn in_scope(&self, prefix: &str) -> Option<&str> {
        for s in self.scopes.iter().rev() {
            if let Some((_, u)) = s.iter().rev().find(|(p, _)| p == prefix) {
                return Some(u);
            }
        }
        if prefix == "xml" {
            return Some(XML_URI);
        }
        None
    }

    fn element(&mut self, n: &Node) {
        let name = n.node_name();
        self.out.push('<');
        self.out.push_str(&name);
        let mut declared: Vec<(String, String)> = Vec::new();
        let mut decls = String::new();
        let mut attrs = String::new();
        let write = |out: &mut String, name: &str, value: &str| {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_attr(value, out);
            out.push('"');
        };
        for a in n.attributes() {
            let an = a.node_name();
            let value = a.node_value().unwrap_or_default();
            if an == "xmlns" || an.starts_with("xmlns:") {
                let prefix = an.strip_prefix("xmlns:").unwrap_or("").to_string();
                if self.in_scope(&prefix) == Some(value.as_str()) && !(prefix.is_empty() && value.is_empty()) {
                    continue;
                }
                declared.push((prefix, value.clone()));
                write(&mut decls, &an, &value);
            } else {
                write(&mut attrs, &an, &value);
            }
        }
        self.scopes.push(declared);
        let mut extra = String::new();
        let ensure = |w: &mut Writer, prefix: &str, uri: &str, extra: &mut String| {
            let bound = w.in_scope(prefix);
            if bound != Some(uri) && !(prefix.is_empty() && uri.is_empty() && bound.is_none()) {
                w.scopes.last_mut().expect("a scope").push((prefix.to_string(), uri.to_string()));
                extra.push_str(if prefix.is_empty() { " xmlns" } else { " xmlns:" });
                extra.push_str(prefix);
                extra.push_str("=\"");
                escape_attr(uri, extra);
                extra.push('"');
            }
        };
        let prefix = n.prefix().unwrap_or_default();
        let uri = n.namespace_uri().unwrap_or_default();
        ensure(self, &prefix, &uri, &mut extra);
        for a in n.attributes() {
            if let (Some(p), Some(u)) = (a.prefix(), a.namespace_uri()) {
                if p != "xmlns" && u != XMLNS_URI {
                    ensure(self, &p, &u, &mut extra);
                }
            }
        }
        self.out.push_str(&decls);
        self.out.push_str(&extra);
        self.out.push_str(&attrs);
        let kids = n.children();
        if kids.is_empty() {
            self.out.push_str("/>");
        } else {
            self.out.push('>');
            for k in &kids {
                self.node(k);
            }
            self.out.push_str("</");
            self.out.push_str(&name);
            self.out.push('>');
        }
        self.scopes.pop();
    }

    fn node(&mut self, n: &Node) {
        match n.node_type() {
            ELEMENT_NODE => self.element(n),
            TEXT_NODE => escape_text(&n.node_value().unwrap_or_default(), &mut self.out),
            CDATA_SECTION_NODE => {
                let v = n.node_value().unwrap_or_default();
                self.out.push_str("<![CDATA[");
                self.out.push_str(&v.replace("]]>", "]]]]><![CDATA[>"));
                self.out.push_str("]]>");
            }
            COMMENT_NODE => {
                self.out.push_str("<!--");
                self.out.push_str(&n.node_value().unwrap_or_default());
                self.out.push_str("-->");
            }
            PROCESSING_INSTRUCTION_NODE => {
                self.out.push_str("<?");
                self.out.push_str(&n.node_name());
                let d = n.node_value().unwrap_or_default();
                if !d.is_empty() {
                    self.out.push(' ');
                    self.out.push_str(&d);
                }
                self.out.push_str("?>");
            }
            DOCUMENT_NODE => {
                for k in n.children() {
                    self.node(&k);
                }
            }
            ATTRIBUTE_NODE => escape_text(&n.node_value().unwrap_or_default(), &mut self.out),
            _ => {}
        }
    }
}

/// `XmlUtils.nodeToString(node)`.
pub fn node_to_string(n: &Node) -> String {
    let mut w = Writer { out: String::new(), scopes: Vec::new() };
    w.node(n);
    w.out
}
