//! A read-only W3C DOM as the JDK's Xerces builds it (namespace aware, non-validating, no external DTD):
//! adjacent character data merged into one `Text` node, CDATA sections, comments and processing instructions
//! kept, attributes ordered by name (Xerces' `NamedNodeMapImpl`), entity references expanded.

use std::collections::HashMap;
use std::sync::Arc;

/// `Node.ELEMENT_NODE`.
pub const ELEMENT_NODE: i32 = 1;
/// `Node.ATTRIBUTE_NODE`.
pub const ATTRIBUTE_NODE: i32 = 2;
/// `Node.TEXT_NODE`.
pub const TEXT_NODE: i32 = 3;
/// `Node.CDATA_SECTION_NODE`.
pub const CDATA_SECTION_NODE: i32 = 4;
/// `Node.PROCESSING_INSTRUCTION_NODE`.
pub const PROCESSING_INSTRUCTION_NODE: i32 = 7;
/// `Node.COMMENT_NODE`.
pub const COMMENT_NODE: i32 = 8;
/// `Node.DOCUMENT_NODE`.
pub const DOCUMENT_NODE: i32 = 9;
/// `Node.DOCUMENT_TYPE_NODE`.
pub const DOCUMENT_TYPE_NODE: i32 = 10;

/// The namespace URI of `xmlns` attributes.
pub const XMLNS_URI: &str = "http://www.w3.org/2000/xmlns/";
/// The namespace URI the `xml` prefix is bound to.
pub const XML_URI: &str = "http://www.w3.org/XML/1998/namespace";

/// One node of a [`Document`], its relatives given by index into the document's nodes.
#[derive(Debug, Clone)]
pub struct NodeData {
    /// `getNodeType()`: one of the `*_NODE` constants.
    pub kind: i32,
    /// `getNodeName()`: the qualified name, `#text`, `#comment`, `#cdata-section`, `#document`, a PI's target.
    pub name: String,
    /// Character data, an attribute's value, a PI's data.
    pub value: String,
    /// `getNamespaceURI()`: `None` for a name in no namespace and for non-element, non-attribute nodes.
    pub namespace_uri: Option<String>,
    /// `getPrefix()`: `None` for an unprefixed name.
    pub prefix: Option<String>,
    /// `getLocalName()`: the name without its prefix; `None` for nodes other than elements and attributes.
    pub local_name: Option<String>,
    /// The parent node (an attribute's owner element).
    pub parent: Option<usize>,
    /// The child nodes in document order.
    pub children: Vec<usize>,
    /// An element's attributes, ordered by name.
    pub attributes: Vec<usize>,
}

/// A parsed document: its nodes in document order (an element's attributes right after it).
#[derive(Debug)]
pub struct Document {
    /// Every node; the document node is at index 0.
    pub nodes: Vec<NodeData>,
}

/// A node of a document.
#[derive(Clone)]
pub struct Node {
    /// The document the node belongs to.
    pub doc: Arc<Document>,
    /// The node's index in the document's nodes.
    pub id: usize,
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        Arc::ptr_eq(&self.doc, &other.doc) && self.id == other.id
    }
}

impl Eq for Node {}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}: {}]", self.node_name(), self.node_value().unwrap_or_else(|| "null".into()))
    }
}

impl Node {
    fn data(&self) -> &NodeData {
        &self.doc.nodes[self.id]
    }

    fn at(&self, id: usize) -> Node {
        Node { doc: self.doc.clone(), id }
    }

    /// `getNodeType()`: one of the `*_NODE` constants.
    pub fn node_type(&self) -> i32 {
        self.data().kind
    }

    /// `getNodeName()`: the qualified name, or `#text`, `#comment`, `#cdata-section`, `#document`, a PI's target.
    pub fn node_name(&self) -> String {
        self.data().name.clone()
    }

    /// `getNodeValue()`: character data and attribute values; `null` for elements and documents.
    pub fn node_value(&self) -> Option<String> {
        match self.node_type() {
            ELEMENT_NODE | DOCUMENT_NODE | DOCUMENT_TYPE_NODE => None,
            _ => Some(self.data().value.clone()),
        }
    }

    /// `getNamespaceURI()`.
    pub fn namespace_uri(&self) -> Option<String> {
        self.data().namespace_uri.clone()
    }

    /// `getPrefix()`.
    pub fn prefix(&self) -> Option<String> {
        self.data().prefix.clone()
    }

    /// `getLocalName()`.
    pub fn local_name(&self) -> Option<String> {
        self.data().local_name.clone()
    }

    /// Whether this is an element.
    pub fn is_element(&self) -> bool {
        self.node_type() == ELEMENT_NODE
    }

    /// Whether this is an attribute.
    pub fn is_attr(&self) -> bool {
        self.node_type() == ATTRIBUTE_NODE
    }

    /// Whether this is a text or CDATA section node.
    pub fn is_text(&self) -> bool {
        matches!(self.node_type(), TEXT_NODE | CDATA_SECTION_NODE)
    }

    /// `getParentNode()` (`null` for attributes, whose owner is [`Node::owner_element`]).
    pub fn parent(&self) -> Option<Node> {
        if self.is_attr() {
            return None;
        }
        self.data().parent.map(|p| self.at(p))
    }

    /// `Attr.getOwnerElement()`.
    pub fn owner_element(&self) -> Option<Node> {
        if !self.is_attr() {
            return None;
        }
        self.data().parent.map(|p| self.at(p))
    }

    /// `getChildNodes()`: in document order.
    pub fn children(&self) -> Vec<Node> {
        self.data().children.iter().map(|c| self.at(*c)).collect()
    }

    /// `getAttributes()`: an element's attributes ordered by name, empty for other nodes.
    pub fn attributes(&self) -> Vec<Node> {
        self.data().attributes.iter().map(|c| self.at(*c)).collect()
    }

    /// `hasAttributes()`.
    pub fn has_attributes(&self) -> bool {
        !self.data().attributes.is_empty()
    }

    /// The attribute named `name` (its qualified name).
    pub fn attribute_node(&self, name: &str) -> Option<Node> {
        self.attributes().into_iter().find(|a| a.data().name == name)
    }

    /// `Element.getAttribute(name)`: `""` when absent.
    pub fn get_attribute(&self, name: &str) -> String {
        self.attribute_node(name).map(|a| a.data().value.clone()).unwrap_or_default()
    }

    /// `Element.hasAttribute(name)`: by qualified name.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attribute_node(name).is_some()
    }

    fn sibling(&self, delta: isize) -> Option<Node> {
        if self.is_attr() {
            return None;
        }
        let parent = self.data().parent?;
        let kids = &self.doc.nodes[parent].children;
        let i = kids.iter().position(|c| *c == self.id)? as isize + delta;
        usize::try_from(i).ok().and_then(|i| kids.get(i)).map(|c| self.at(*c))
    }

    /// `getPreviousSibling()`: `None` for the first child and for attributes.
    pub fn previous_sibling(&self) -> Option<Node> {
        self.sibling(-1)
    }

    /// `getNextSibling()`: `None` for the last child and for attributes.
    pub fn next_sibling(&self) -> Option<Node> {
        self.sibling(1)
    }

    /// `getFirstChild()`: `None` without children.
    pub fn first_child(&self) -> Option<Node> {
        self.data().children.first().map(|c| self.at(*c))
    }

    /// `getLastChild()`: `None` without children.
    pub fn last_child(&self) -> Option<Node> {
        self.data().children.last().map(|c| self.at(*c))
    }

    /// `getOwnerDocument()`: the document node.
    pub fn owner_document(&self) -> Node {
        self.at(0)
    }

    /// `Document.getDocumentElement()`.
    pub fn document_element(&self) -> Option<Node> {
        self.at(0).children().into_iter().find(Node::is_element)
    }

    /// `getTextContent()`: character data of the descendants (comments and PIs excluded); `null` for documents.
    pub fn text_content(&self) -> Option<String> {
        match self.node_type() {
            DOCUMENT_NODE | DOCUMENT_TYPE_NODE => None,
            ELEMENT_NODE => {
                let mut out = String::new();
                self.collect_text(&mut out);
                Some(out)
            }
            _ => Some(self.data().value.clone()),
        }
    }

    fn collect_text(&self, out: &mut String) {
        for c in self.children() {
            match c.node_type() {
                TEXT_NODE | CDATA_SECTION_NODE => out.push_str(&c.data().value),
                ELEMENT_NODE => c.collect_text(out),
                _ => {}
            }
        }
    }

    /// `Node.toString()` of Xerces' nodes: `[name: value]`.
    pub fn java_string(&self) -> String {
        format!("[{}: {}]", self.node_name(), self.node_value().unwrap_or_else(|| "null".into()))
    }

    /// The DOM interface a node implements, for class names.
    pub fn class_name(&self) -> &'static str {
        match self.node_type() {
            ELEMENT_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredElementNSImpl",
            ATTRIBUTE_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredAttrNSImpl",
            TEXT_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredTextImpl",
            CDATA_SECTION_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredCDATASectionImpl",
            COMMENT_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredCommentImpl",
            PROCESSING_INSTRUCTION_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredProcessingInstructionImpl",
            DOCUMENT_TYPE_NODE => "com.sun.org.apache.xerces.internal.dom.DeferredDocumentTypeImpl",
            _ => "com.sun.org.apache.xerces.internal.dom.DeferredDocumentImpl",
        }
    }

    /// The in-scope namespace of `prefix` (`None` prefix: the default namespace).
    pub fn lookup_namespace(&self, prefix: Option<&str>) -> Option<String> {
        let mut n = if self.is_attr() { self.owner_element() } else { Some(self.clone()) };
        while let Some(e) = n {
            if e.is_element() {
                for a in e.attributes() {
                    let d = a.data();
                    let declares = match prefix {
                        None => d.name == "xmlns",
                        Some(p) => d.prefix.as_deref() == Some("xmlns") && d.local_name.as_deref() == Some(p),
                    };
                    if declares {
                        return (!d.value.is_empty()).then(|| d.value.clone());
                    }
                }
            }
            n = e.parent();
        }
        match prefix {
            Some("xml") => Some(XML_URI.into()),
            Some("xmlns") => Some(XMLNS_URI.into()),
            _ => None,
        }
    }
}

/// A well-formedness error, as the SAX parse exception reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    /// The parser's message.
    pub message: String,
    /// The 1-based line of the error.
    pub line: usize,
    /// The 1-based column of the error.
    pub column: usize,
}

struct Builder {
    nodes: Vec<NodeData>,
}

impl Builder {
    fn add(&mut self, data: NodeData) -> usize {
        let id = self.nodes.len();
        if let Some(p) = data.parent {
            if data.kind == ATTRIBUTE_NODE {
                self.nodes[p].attributes.push(id);
            } else {
                self.nodes[p].children.push(id);
            }
        }
        self.nodes.push(data);
        id
    }
}

fn node(kind: i32, name: &str, value: String, parent: Option<usize>) -> NodeData {
    NodeData { kind, name: name.into(), value, namespace_uri: None, prefix: None, local_name: None, parent, children: Vec::new(), attributes: Vec::new() }
}

fn is_name_start(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// An element being built: its node, its raw attributes and the namespace declarations it makes.
struct Open {
    id: usize,
    name: String,
    scope: HashMap<Option<String>, Option<String>>,
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    b: Builder,
    entities: HashMap<String, String>,
    stack: Vec<Open>,
    text: String,
    _src: &'a str,
    depth: usize,
}

type R<T> = Result<T, ParseError>;

impl<'a> Parser<'a> {
    fn error<T>(&self, message: impl Into<String>) -> R<T> {
        let (mut line, mut column) = (1, 1);
        for c in &self.chars[..self.pos.min(self.chars.len())] {
            if *c == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        Err(ParseError { message: message.into(), line, column })
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn starts(&self, s: &str) -> bool {
        for (i, c) in (self.pos..).zip(s.chars()) {
            if self.chars.get(i) != Some(&c) {
                return false;
            }
        }
        true
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.starts(s) {
            self.pos += s.chars().count();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn name(&mut self) -> R<String> {
        match self.peek() {
            Some(c) if is_name_start(c) => {}
            _ => return self.error("A name was expected."),
        }
        let start = self.pos;
        while self.peek().is_some_and(is_name_char) {
            self.pos += 1;
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn current_parent(&self) -> usize {
        self.stack.last().map(|o| o.id).unwrap_or(0)
    }

    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let t = std::mem::take(&mut self.text);
            let parent = self.current_parent();
            self.b.add(node(TEXT_NODE, "#text", t, Some(parent)));
        }
    }

    /// `&…;` after the `&`: a character or a predefined / declared entity's replacement text.
    fn reference(&mut self) -> R<Reference> {
        if self.eat("#x") {
            let start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            let digits: String = self.chars[start..self.pos].iter().collect();
            if digits.is_empty() || !self.eat(";") {
                return self.error("A decimal representation must immediately follow the \"&#\" in a character reference.");
            }
            return self.char_ref(u32::from_str_radix(&digits, 16).ok());
        }
        if self.eat("#") {
            let start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
            let digits: String = self.chars[start..self.pos].iter().collect();
            if digits.is_empty() || !self.eat(";") {
                return self.error("A decimal representation must immediately follow the \"&#\" in a character reference.");
            }
            return self.char_ref(digits.parse().ok());
        }
        let name = self.name()?;
        if !self.eat(";") {
            return self.error(format!("The reference to entity \"{name}\" must end with the ';' delimiter."));
        }
        Ok(match name.as_str() {
            "lt" => Reference::Char('<'),
            "gt" => Reference::Char('>'),
            "amp" => Reference::Char('&'),
            "apos" => Reference::Char('\''),
            "quot" => Reference::Char('"'),
            _ => match self.entities.get(&name) {
                Some(v) => Reference::Entity(name, v.clone()),
                None => return self.error(format!("The entity \"{name}\" was referenced, but not declared.")),
            },
        })
    }

    fn char_ref(&self, v: Option<u32>) -> R<Reference> {
        match v.and_then(char::from_u32).filter(|c| is_xml_char(*c)) {
            Some(c) => Ok(Reference::Char(c)),
            None => self.error("Character reference is an invalid XML character."),
        }
    }

    /// An attribute value between quotes, normalized (whitespace characters become spaces).
    fn attribute_value(&mut self) -> R<String> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            _ => return self.error("Open quote is expected for attribute."),
        };
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return self.error("XML document structures must start and end within the same entity."),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('<') => return self.error("The value of attribute must not contain the '<' character."),
                Some('&') => {
                    self.pos += 1;
                    match self.reference()? {
                        Reference::Char(c) => out.push(c),
                        Reference::Entity(name, text) => {
                            self.depth += 1;
                            if self.depth > 64 || text.contains(&format!("&{name};")) {
                                return self.error(format!("Recursive entity reference \"{name}\"."));
                            }
                            let expanded = self.expand_attribute_text(&text)?;
                            self.depth -= 1;
                            out.push_str(&expanded);
                        }
                    }
                }
                Some(c) if is_space(c) => {
                    self.pos += 1;
                    out.push(' ');
                }
                Some(c) if !is_xml_char(c) => return self.error(format!("An invalid XML character (Unicode: 0x{:x}) was found in the value of attribute.", c as u32)),
                Some(c) => {
                    self.pos += 1;
                    out.push(c);
                }
            }
        }
    }

    fn expand_attribute_text(&mut self, text: &str) -> R<String> {
        let mut sub = Parser { chars: text.chars().collect(), pos: 0, b: Builder { nodes: Vec::new() }, entities: self.entities.clone(), stack: Vec::new(), text: String::new(), _src: self._src, depth: self.depth };
        let mut out = String::new();
        while let Some(c) = sub.peek() {
            match c {
                '<' => return self.error("The value of attribute must not contain the '<' character."),
                '&' => {
                    sub.pos += 1;
                    match sub.reference()? {
                        Reference::Char(c) => out.push(c),
                        Reference::Entity(_, t) => {
                            sub.depth += 1;
                            if sub.depth > 64 {
                                return self.error("Recursive entity reference.");
                            }
                            out.push_str(&sub.expand_attribute_text(&t)?);
                        }
                    }
                }
                c if is_space(c) => {
                    sub.pos += 1;
                    out.push(' ');
                }
                c => {
                    sub.pos += 1;
                    out.push(c);
                }
            }
        }
        Ok(out)
    }

    fn comment(&mut self) -> R<()> {
        let start = self.pos;
        loop {
            if self.starts("--") {
                if !self.starts("-->") {
                    return self.error("The string \"--\" is not permitted within comments.");
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                self.pos += 3;
                self.flush_text();
                let parent = self.current_parent();
                self.b.add(node(COMMENT_NODE, "#comment", text, Some(parent)));
                return Ok(());
            }
            match self.peek() {
                None => return self.error("XML document structures must start and end within the same entity."),
                Some(c) if !is_xml_char(c) => return self.error(format!("An invalid XML character (Unicode: 0x{:x}) was found in the comment.", c as u32)),
                _ => self.pos += 1,
            }
        }
    }

    fn processing_instruction(&mut self) -> R<()> {
        let target = self.name()?;
        if target.eq_ignore_ascii_case("xml") {
            return self.error("The processing instruction target matching \"[xX][mM][lL]\" is not allowed.");
        }
        let data = if self.eat("?>") {
            String::new()
        } else {
            if !self.skip_space() {
                return self.error("White space is required between the processing instruction target and data.");
            }
            let start = self.pos;
            while !self.starts("?>") {
                if self.peek().is_none() {
                    return self.error("XML document structures must start and end within the same entity.");
                }
                self.pos += 1;
            }
            let d: String = self.chars[start..self.pos].iter().collect();
            self.pos += 2;
            d
        };
        self.flush_text();
        let parent = self.current_parent();
        self.b.add(node(PROCESSING_INSTRUCTION_NODE, &target, data, Some(parent)));
        Ok(())
    }

    fn cdata(&mut self) -> R<()> {
        let start = self.pos;
        while !self.starts("]]>") {
            match self.peek() {
                None => return self.error("The CDATA section must end with ']]>'."),
                Some(c) if !is_xml_char(c) => return self.error(format!("An invalid XML character (Unicode: 0x{:x}) was found in the CDATA section.", c as u32)),
                _ => self.pos += 1,
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        self.pos += 3;
        self.flush_text();
        let parent = self.current_parent();
        self.b.add(node(CDATA_SECTION_NODE, "#cdata-section", text, Some(parent)));
        Ok(())
    }

    /// `<!DOCTYPE …>`: the internal subset's general entities are remembered; nothing external is read.
    fn doctype(&mut self) -> R<()> {
        if !self.skip_space() {
            return self.error("White space is required after \"<!DOCTYPE\" in the document type declaration.");
        }
        let name = self.name()?;
        self.skip_space();
        let quoted = |p: &mut Parser| -> R<String> {
            let q = match p.peek() {
                Some(q @ ('"' | '\'')) => q,
                _ => return p.error("The system identifier must begin with either a single or double quote character."),
            };
            p.pos += 1;
            let start = p.pos;
            while p.peek().is_some_and(|c| c != q) {
                p.pos += 1;
            }
            if p.peek().is_none() {
                return p.error("XML document structures must start and end within the same entity.");
            }
            let v: String = p.chars[start..p.pos].iter().collect();
            p.pos += 1;
            Ok(v)
        };
        if self.eat("SYSTEM") {
            self.skip_space();
            quoted(self)?;
        } else if self.eat("PUBLIC") {
            self.skip_space();
            quoted(self)?;
            self.skip_space();
            quoted(self)?;
        }
        self.skip_space();
        if self.eat("[") {
            loop {
                self.skip_space();
                if self.eat("]") {
                    break;
                }
                if self.eat("<!ENTITY") {
                    self.skip_space();
                    let parameter = self.eat("%");
                    self.skip_space();
                    let ename = self.name()?;
                    self.skip_space();
                    if matches!(self.peek(), Some('"' | '\'')) {
                        let q = self.peek().expect("a quote");
                        self.pos += 1;
                        let start = self.pos;
                        while self.peek().is_some_and(|c| c != q) {
                            self.pos += 1;
                        }
                        let raw: String = self.chars[start..self.pos].iter().collect();
                        self.pos += 1;
                        let value = self.expand_char_refs(&raw)?;
                        if !parameter && !self.entities.contains_key(&ename) {
                            self.entities.insert(ename.clone(), value);
                        }
                    } else {
                        while self.peek().is_some_and(|c| c != '>') {
                            self.pos += 1;
                        }
                    }
                    self.skip_space();
                    if !self.eat(">") {
                        return self.error(format!("The declaration for the entity \"{ename}\" must end with '>'."));
                    }
                } else if self.eat("<!--") {
                    let start = self.pos;
                    while !self.starts("-->") && self.peek().is_some() {
                        self.pos += 1;
                    }
                    let _ = start;
                    self.pos += 3;
                } else if self.eat("<?") {
                    while !self.starts("?>") && self.peek().is_some() {
                        self.pos += 1;
                    }
                    self.pos += 2;
                } else if self.eat("<!") {
                    let mut quote: Option<char> = None;
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        match quote {
                            Some(q) if c == q => quote = None,
                            Some(_) => {}
                            None if c == '"' || c == '\'' => quote = Some(c),
                            None if c == '>' => break,
                            None => {}
                        }
                    }
                } else if self.eat("%") {
                    self.name()?;
                    self.eat(";");
                } else {
                    return self.error("The markup declarations contained or pointed to by the document type declaration must be well-formed.");
                }
            }
            self.skip_space();
        }
        if !self.eat(">") {
            return self.error(format!("The document type declaration for root element type \"{name}\" must end with '>'."));
        }
        self.b.add(node(DOCUMENT_TYPE_NODE, &name, String::new(), Some(0)));
        Ok(())
    }

    /// An entity value's character references (general entity references stay for expansion at use).
    fn expand_char_refs(&self, raw: &str) -> R<String> {
        let mut out = String::new();
        let mut rest = raw;
        while let Some(i) = rest.find("&#") {
            out.push_str(&rest[..i]);
            let after = &rest[i + 2..];
            let Some(end) = after.find(';') else { return self.error("A decimal representation must immediately follow the \"&#\" in a character reference.") };
            let code = &after[..end];
            let v = match code.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => code.parse().ok(),
            };
            match v.and_then(char::from_u32) {
                Some(c) => out.push(c),
                None => return self.error("Character reference is an invalid XML character."),
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }

    fn start_tag(&mut self) -> R<()> {
        let name = self.name()?;
        let mut raw: Vec<(String, String)> = Vec::new();
        loop {
            let spaced = self.skip_space();
            match self.peek() {
                Some('>') => {
                    self.pos += 1;
                    self.open(name, raw, false)?;
                    return Ok(());
                }
                Some('/') => {
                    self.pos += 1;
                    if !self.eat(">") {
                        return self.error(format!("Element type \"{name}\" must be followed by either attribute specifications, \">\" or \"/>\"."));
                    }
                    self.open(name, raw, true)?;
                    return Ok(());
                }
                None => return self.error("XML document structures must start and end within the same entity."),
                Some(_) => {
                    if !spaced {
                        return self.error(format!("Element type \"{name}\" must be followed by either attribute specifications, \">\" or \"/>\"."));
                    }
                    let attr = self.name()?;
                    self.skip_space();
                    if !self.eat("=") {
                        return self.error(format!("Attribute name \"{attr}\" associated with an element type \"{name}\" must be followed by the ' = ' character."));
                    }
                    self.skip_space();
                    let v = self.attribute_value()?;
                    if raw.iter().any(|(n, _)| *n == attr) {
                        return self.error(format!("Attribute \"{attr}\" was already specified for element \"{name}\"."));
                    }
                    raw.push((attr, v));
                }
            }
        }
    }

    fn resolve(&self, scope: &HashMap<Option<String>, Option<String>>, prefix: Option<&str>) -> Option<Option<String>> {
        let key = prefix.map(str::to_string);
        if let Some(v) = scope.get(&key) {
            return Some(v.clone());
        }
        for o in self.stack.iter().rev() {
            if let Some(v) = o.scope.get(&key) {
                return Some(v.clone());
            }
        }
        match prefix {
            None => Some(None),
            Some("xml") => Some(Some(XML_URI.into())),
            _ => None,
        }
    }

    fn open(&mut self, name: String, raw: Vec<(String, String)>, empty: bool) -> R<()> {
        self.flush_text();
        if self.stack.is_empty() && self.b.nodes[0].children.iter().any(|c| self.b.nodes[*c].kind == ELEMENT_NODE) {
            return self.error("The markup in the document following the root element must be well-formed.");
        }
        let split = |q: &str| -> (Option<String>, String) {
            match q.split_once(':') {
                Some((p, l)) if !p.is_empty() && !l.is_empty() => (Some(p.to_string()), l.to_string()),
                _ => (None, q.to_string()),
            }
        };
        let mut scope: HashMap<Option<String>, Option<String>> = HashMap::new();
        for (n, v) in &raw {
            if n == "xmlns" {
                scope.insert(None, (!v.is_empty()).then(|| v.clone()));
            } else if let Some(p) = n.strip_prefix("xmlns:") {
                if v.is_empty() {
                    return self.error(format!("The value of the attribute \"prefix=\"xmlns\",localpart=\"{p}\",rawname=\"{n}\"\" is invalid. Prefixed namespace bindings may not be empty."));
                }
                scope.insert(Some(p.to_string()), Some(v.clone()));
            }
        }
        let (prefix, local) = split(&name);
        let Some(uri) = self.resolve(&scope, prefix.as_deref()) else {
            return self.error(format!("The prefix \"{}\" for element \"{name}\" is not bound.", prefix.unwrap_or_default()));
        };
        let parent = self.current_parent();
        let mut data = node(ELEMENT_NODE, &name, String::new(), Some(parent));
        data.namespace_uri = uri;
        data.prefix = prefix;
        data.local_name = Some(local);
        let id = self.b.add(data);
        let mut attrs: Vec<NodeData> = Vec::new();
        for (n, v) in raw {
            let (ap, al) = split(&n);
            let uri = if n == "xmlns" || ap.as_deref() == Some("xmlns") {
                Some(XMLNS_URI.to_string())
            } else if let Some(p) = ap.as_deref() {
                match self.resolve(&scope, Some(p)) {
                    Some(u) => u,
                    None => return self.error(format!("The prefix \"{p}\" for attribute \"{n}\" associated with an element type \"{name}\" is not bound.")),
                }
            } else {
                None
            };
            let mut a = node(ATTRIBUTE_NODE, &n, v, Some(id));
            a.namespace_uri = uri;
            a.prefix = ap;
            a.local_name = Some(al);
            attrs.push(a);
        }
        attrs.sort_by(|x, y| crate::tools::xml::java_compare(&x.name, &y.name));
        for a in attrs {
            self.b.add(a);
        }
        if !empty {
            self.stack.push(Open { id, name, scope });
        } else if self.stack.is_empty() {
            self.stack.push(Open { id, name, scope });
            self.stack.pop();
        }
        Ok(())
    }

    fn end_tag(&mut self) -> R<()> {
        let name = self.name()?;
        self.skip_space();
        if !self.eat(">") {
            return self.error(format!("The end-tag for element type \"{name}\" must end with a '>' delimiter."));
        }
        match self.stack.last() {
            Some(o) if o.name == name => {
                self.flush_text();
                self.stack.pop();
                Ok(())
            }
            Some(o) => {
                let open = o.name.clone();
                self.error(format!("The element type \"{open}\" must be terminated by the matching end-tag \"</{open}>\"."))
            }
            None => self.error("The markup in the document following the root element must be well-formed."),
        }
    }

    /// Content (or the prolog / epilog when no element is open).
    fn content(&mut self) -> R<()> {
        while let Some(c) = self.peek() {
            let top = self.stack.is_empty();
            if c == '<' {
                self.pos += 1;
                if self.eat("!--") {
                    self.comment()?;
                } else if self.eat("![CDATA[") {
                    if top {
                        return self.error("The markup in the document following the root element must be well-formed.");
                    }
                    self.cdata()?;
                } else if self.eat("!DOCTYPE") {
                    if !top || self.b.nodes[0].children.iter().any(|c| matches!(self.b.nodes[*c].kind, ELEMENT_NODE | DOCUMENT_TYPE_NODE)) {
                        return self.error("The markup in the document preceding the root element must be well-formed.");
                    }
                    self.doctype()?;
                } else if self.eat("?") {
                    self.processing_instruction()?;
                } else if self.eat("/") {
                    self.end_tag()?;
                } else {
                    self.start_tag()?;
                }
            } else if top {
                if !is_space(c) {
                    let rooted = self.b.nodes[0].children.iter().any(|c| self.b.nodes[*c].kind == ELEMENT_NODE);
                    return self.error(if rooted { "Content is not allowed in trailing section." } else { "Content is not allowed in prolog." });
                }
                self.pos += 1;
            } else if c == '&' {
                self.pos += 1;
                match self.reference()? {
                    Reference::Char(c) => self.text.push(c),
                    Reference::Entity(name, text) => self.entity_content(&name, &text)?,
                }
            } else if self.starts("]]>") {
                return self.error("The character sequence \"]]>\" must not appear in content unless used to mark the end of a CDATA section.");
            } else if !is_xml_char(c) {
                return self.error(format!("An invalid XML character (Unicode: 0x{:x}) was found in the element content of the document.", c as u32));
            } else {
                self.pos += 1;
                self.text.push(c);
            }
        }
        Ok(())
    }

    /// A general entity in content: its replacement text parsed as content in place.
    fn entity_content(&mut self, name: &str, text: &str) -> R<()> {
        self.depth += 1;
        if self.depth > 64 {
            return self.error(format!("Recursive entity reference \"{name}\"."));
        }
        let saved_chars = std::mem::replace(&mut self.chars, text.chars().collect());
        let saved_pos = std::mem::replace(&mut self.pos, 0);
        let depth = self.stack.len();
        let r = self.content();
        let balanced = self.stack.len() == depth;
        self.chars = saved_chars;
        self.pos = saved_pos;
        self.depth -= 1;
        r?;
        if !balanced {
            return self.error(format!("The replacement text of entity \"{name}\" must be well-formed."));
        }
        Ok(())
    }

    fn document(&mut self) -> R<()> {
        if self.peek() == Some('\u{feff}') {
            self.pos += 1;
        }
        if self.starts("<?xml") && self.chars.get(self.pos + 5).is_some_and(|c| is_space(*c)) {
            self.pos += 5;
            while !self.starts("?>") {
                if self.peek().is_none() {
                    return self.error("XML document structures must start and end within the same entity.");
                }
                self.pos += 1;
            }
            self.pos += 2;
        }
        self.content()?;
        if let Some(o) = self.stack.last() {
            let name = o.name.clone();
            return self.error(format!("XML document structures must start and end within the same entity. (element \"{name}\")"));
        }
        if !self.b.nodes[0].children.iter().any(|c| self.b.nodes[*c].kind == ELEMENT_NODE) {
            return self.error("Premature end of file.");
        }
        Ok(())
    }
}

enum Reference {
    Char(char),
    Entity(String, String),
}

/// `DocumentBuilder.parse`: the document node, or the first well-formedness error.
pub fn parse(xml: &str) -> Result<Node, ParseError> {
    let normalized = xml.replace("\r\n", "\n").replace('\r', "\n");
    let mut p = Parser {
        chars: normalized.chars().collect(),
        pos: 0,
        b: Builder { nodes: vec![node(DOCUMENT_NODE, "#document", String::new(), None)] },
        entities: HashMap::new(),
        stack: Vec::new(),
        text: String::new(),
        _src: xml,
        depth: 0,
    };
    if p.chars.is_empty() {
        return Err(ParseError { message: "Premature end of file.".into(), line: 1, column: 1 });
    }
    p.document()?;
    Ok(Node { doc: Arc::new(Document { nodes: p.b.nodes }), id: 0 })
}
