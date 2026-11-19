//! `org.apache.velocity.tools.generic.MarkupTool` (`$mark`): HTML tags from CSS-like definitions
//! (`div#id.class[attr=value] span`), nested and rendered with indentation.

use std::sync::{Arc, Weak};

use java_compat::lang::{JList, JMap, JObject, JValue, ListKind, MapKind, java_equals};
use java_compat::reflect::{ClassDef, JType};
use parking_lot::Mutex;

use super::classes::{Annotation, JavaClass};
use super::safe::{self, SafeConfig, Tool};
use super::{c, identity, o, s, tool_object, val};

/// The class name of `org.apache.velocity.tools.generic.MarkupTool`.
pub const CLASS: &str = "org.apache.velocity.tools.generic.MarkupTool";
/// The class name of `MarkupTool.Tag`.
pub const TAG: &str = "org.apache.velocity.tools.generic.MarkupTool$Tag";
/// `MarkupTool.DEFAULT_TAB`: two spaces.
pub const DEFAULT_TAB: &str = "  ";
/// `MarkupTool.DEFAULT_DELIMITER`: the space that separates nested tag definitions.
pub const DEFAULT_DELIMITER: &str = " ";

/// Port of `org.apache.velocity.tools.generic.MarkupTool` (`$markup`): HTML/XML tags built from CSS-like definitions
/// such as `div#id.class[attr=value]`.
pub struct MarkupTool {
    safe: SafeConfig,
    tab: Mutex<String>,
    me: Weak<MarkupTool>,
}

#[derive(Clone, Copy)]
enum Mode {
    Name,
    Id,
    Class,
    Attr,
}

impl MarkupTool {
    /// `new MarkupTool()`: indenting with the default tab.
    pub fn new() -> Arc<MarkupTool> {
        Arc::new_cyclic(|me| MarkupTool { safe: SafeConfig::default(), tab: Mutex::new(DEFAULT_TAB.into()), me: me.clone() })
    }

    /// This tool as a template value.
    pub fn value(&self) -> JValue {
        self.me.upgrade().map(|t| JValue::Object(t as Arc<dyn JObject>)).unwrap_or(JValue::Null)
    }

    /// `SafeConfig.configure(Map)`: nothing once the configuration is locked.
    pub fn configure(&self, params: &JMap) {
        safe::configure(self, params)
    }

    /// `setTab(tab)`: refused (logged) once the configuration is locked.
    pub fn set_tab(&self, tab: &str) {
        if self.safe.is_config_locked() {
            super::log::error(CLASS, "setTab() failure: configuration is locked");
        } else {
            *self.tab.lock() = tab.to_string();
        }
    }

    /// `getTab()`: the indentation one nesting level adds.
    pub fn get_tab(&self) -> String {
        self.tab.lock().clone()
    }

    /// `tag(definition)`: each space-separated definition nested in the previous one; the innermost returned.
    pub fn tag(&self, definition: &str) -> Option<Arc<Tag>> {
        let tags = java_compat::methods::split(definition, DEFAULT_DELIMITER, 0).unwrap_or_default();
        let mut last: Option<Arc<Tag>> = None;
        for t in tags {
            let tag = self.parse(&t);
            if let Some(l) = &last {
                l.append(&tag.value());
            }
            last = Some(tag);
        }
        last
    }

    fn parse(&self, definition: &str) -> Arc<Tag> {
        let tag = Tag::new(self.me.upgrade().expect("alive"));
        let mut store = String::new();
        let mut mode = Mode::Name;
        for ch in definition.chars() {
            let next = match ch {
                '#' => Some(Mode::Id),
                '.' => Some(Mode::Class),
                '[' => Some(Mode::Attr),
                ']' => Some(Mode::Name),
                _ => None,
            };
            match next {
                Some(m) => {
                    clear(mode, &tag, &mut store, true);
                    mode = m;
                }
                None => store.push(ch),
            }
        }
        clear(mode, &tag, &mut store, false);
        tag
    }

    fn java_string(&self) -> String {
        identity(self)
    }
}

fn clear(mode: Mode, tag: &Tag, store: &mut String, empty: bool) {
    if store.is_empty() {
        return;
    }
    let v = store.clone();
    match mode {
        Mode::Name => {
            tag.name(JValue::str(v));
        }
        Mode::Id => {
            tag.id(JValue::str(v));
        }
        Mode::Class => {
            tag.add_class(JValue::str(v));
        }
        Mode::Attr => {
            if v.find('=').is_some_and(|i| i > 0) {
                let kv = java_compat::methods::split(&v, "=", 0).unwrap_or_default();
                tag.attr(JValue::str(kv[0].as_str()), kv.get(1).map(|x| JValue::str(x.as_str())).unwrap_or(JValue::Null));
            } else {
                tag.attr(JValue::str(v), JValue::Null);
            }
        }
    }
    if empty {
        store.clear();
    }
}

impl Tool for MarkupTool {
    fn safe(&self) -> &SafeConfig {
        &self.safe
    }
}

struct TagState {
    parent: Option<Arc<Tag>>,
    name: JValue,
    id: JValue,
    classes: Option<Vec<JValue>>,
    attributes: Option<JMap>,
    children: Option<Vec<JValue>>,
}

/// `MarkupTool.Tag` (a parent keeps its children and a child its parent, as in Java).
pub struct Tag {
    tool: Arc<MarkupTool>,
    st: Mutex<TagState>,
    me: Weak<Tag>,
}

impl Tag {
    /// A tag with no name, id, classes, attributes, children or parent, built by `tool`.
    pub fn new(tool: Arc<MarkupTool>) -> Arc<Tag> {
        Arc::new_cyclic(|me| Tag {
            tool,
            st: Mutex::new(TagState { parent: None, name: JValue::Null, id: JValue::Null, classes: None, attributes: None, children: None }),
            me: me.clone(),
        })
    }

    fn this(&self) -> Arc<Tag> {
        self.me.upgrade().expect("alive")
    }

    /// This tag as a template value.
    pub fn value(&self) -> JValue {
        JValue::Object(self.this() as Arc<dyn JObject>)
    }

    /// `name(name)`: sets the element name; this tag.
    pub fn name(&self, name: JValue) -> Arc<Tag> {
        self.st.lock().name = name;
        self.this()
    }

    /// `id(id)`: sets the `id` attribute; this tag.
    pub fn id(&self, id: JValue) -> Arc<Tag> {
        self.st.lock().id = id;
        self.this()
    }

    /// `addClass(c)`: appends a class; this tag, `None` for a `null` class.
    pub fn add_class(&self, c: JValue) -> Option<Arc<Tag>> {
        if c.is_null() {
            return None;
        }
        self.st.lock().classes.get_or_insert_with(Vec::new).push(c);
        Some(self.this())
    }

    /// `attr(k, v)`: sets an attribute (a `null` value prints the bare name); this tag, `None` for a `null` key.
    pub fn attr(&self, k: JValue, v: JValue) -> Option<Arc<Tag>> {
        if k.is_null() {
            return None;
        }
        self.st.lock().attributes.get_or_insert_with(|| JMap::new(MapKind::Hash)).put(k, v);
        Some(self.this())
    }

    /// `body(o)`: replaces the children with `o`; this tag.
    pub fn body(&self, o: JValue) -> Arc<Tag> {
        let mut st = self.st.lock();
        let children = st.children.get_or_insert_with(Vec::new);
        children.clear();
        children.push(o);
        drop(st);
        self.this()
    }

    /// `append(o)`: adds `o` as the last child (becoming its parent when it is a tag); this tag.
    pub fn append(&self, o: &JValue) -> Arc<Tag> {
        self.st.lock().children.get_or_insert_with(Vec::new).push(o.clone());
        if let Some(t) = as_tag(o) {
            t.set_parent(Some(self.this()));
        }
        self.this()
    }

    /// `prepend(o)`: adds `o` as the first child (becoming its parent when it is a tag); this tag.
    pub fn prepend(&self, o: &JValue) -> Arc<Tag> {
        self.st.lock().children.get_or_insert_with(Vec::new).insert(0, o.clone());
        if let Some(t) = as_tag(o) {
            t.set_parent(Some(self.this()));
        }
        self.this()
    }

    /// `wrap(tag)`: the new tags take this tag's place under its parent.
    pub fn wrap(&self, tag: &str) -> Option<Arc<Tag>> {
        let prnt = self.tool.tag(tag)?;
        prnt.root().set_parent(self.parent());
        self.set_parent(Some(prnt));
        Some(self.this())
    }

    /// `orphan()`: forgets the parent; this tag.
    pub fn orphan(&self) -> Arc<Tag> {
        self.set_parent(None)
    }

    /// `parent(tag)`: sets the parent without adding this tag to its children; this tag.
    pub fn set_parent(&self, parent: Option<Arc<Tag>>) -> Arc<Tag> {
        self.st.lock().parent = parent;
        self.this()
    }

    /// `parent()`: `None` for an orphan.
    pub fn parent(&self) -> Option<Arc<Tag>> {
        self.st.lock().parent.clone()
    }

    /// `root()`: the topmost ancestor, this tag for an orphan.
    pub fn root(&self) -> Arc<Tag> {
        match self.parent() {
            None => self.this(),
            Some(p) => p.root(),
        }
    }

    /// `children()`: `None` until a child is added.
    pub fn children(&self) -> Option<Vec<JValue>> {
        self.st.lock().children.clone()
    }

    /// `isOrphan()`: whether the tag has no parent.
    pub fn is_orphan(&self) -> bool {
        self.parent().is_none()
    }

    /// `isEmpty()`: whether the tag has no children.
    pub fn is_empty(&self) -> bool {
        self.st.lock().children.as_ref().is_none_or(|c| c.is_empty())
    }

    /// `matches(tag)`: the other's name, id and classes (those it has) are this tag's too.
    pub fn matches(&self, tag: &Tag) -> bool {
        let (a, b) = (self.st.lock(), tag.st.lock());
        let missed = |target: &JValue, arrow: &JValue| !arrow.is_null() && !java_equals(arrow, target);
        let missed_list = |targets: &Option<Vec<JValue>>, arrows: &Option<Vec<JValue>>| match (targets, arrows) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(t), Some(a)) => a.iter().any(|o| !t.iter().any(|x| java_equals(x, o))),
        };
        !(missed(&a.name, &b.name) || missed(&a.id, &b.id) || missed_list(&a.classes, &b.classes))
    }

    fn render(&self, indent: Option<&str>, out: &mut String) {
        if self.render_start(indent, out) {
            self.render_body(indent, out);
            self.render_end(indent, out);
        }
    }

    fn render_start(&self, indent: Option<&str>, out: &mut String) -> bool {
        if let Some(i) = indent {
            out.push_str(i);
        }
        out.push('<');
        let st = self.st.lock();
        out.push_str(&if st.name.is_null() { "div".to_string() } else { st.name.to_java_string() });
        if !st.id.is_null() {
            out.push_str(&format!(" id=\"{}\"", st.id.to_java_string()));
        }
        if let Some(classes) = &st.classes {
            let joined: Vec<String> = classes.iter().map(JValue::to_java_string).collect();
            out.push_str(&format!(" class=\"{}\"", joined.join(" ")));
        }
        if let Some(attrs) = &st.attributes {
            for (k, v) in attrs.entries() {
                out.push_str(&format!(" {}=\"{}\"", k.to_java_string(), if v.is_null() { String::new() } else { v.to_java_string() }));
            }
        }
        let empty = st.children.as_ref().is_none_or(|c| c.is_empty());
        out.push_str(if empty { "/>" } else { ">" });
        !empty
    }

    fn render_body(&self, indent: Option<&str>, out: &mut String) {
        let kid_indent = format!("{}{}", indent.unwrap_or("null"), self.tool.get_tab());
        for o in self.children().unwrap_or_default() {
            match as_tag(&o) {
                Some(t) => t.render(Some(&kid_indent), out),
                None => {
                    out.push_str(&kid_indent);
                    out.push_str(&o.to_java_string());
                }
            }
        }
    }

    fn render_end(&self, indent: Option<&str>, out: &mut String) {
        if let Some(i) = indent {
            out.push_str(i);
        }
        out.push_str(&format!("</{}>", self.st.lock().name.to_java_string()));
    }

    /// `toString()`: the whole tree from the root, each tag on its own line.
    pub fn java_string(&self) -> String {
        let mut out = String::new();
        self.root().render(Some("\n"), &mut out);
        out
    }
}

fn as_tag(v: &JValue) -> Option<Arc<Tag>> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<Tag>().and_then(|t| t.me.upgrade()),
        _ => None,
    }
}

fn opt(t: Option<Arc<Tag>>) -> super::Ret {
    val(t.map(|t| t.value()).unwrap_or(JValue::Null))
}

fn tag_def() -> Arc<ClassDef> {
    type T = Tag;
    let me = || c(TAG);
    let def = ClassDef::new(TAG)
        .method::<T>("name", &[o()], Some(me()), |t, _, a| opt(Some(t.name(a[0].clone()))))
        .method::<T>("id", &[o()], Some(me()), |t, _, a| opt(Some(t.id(a[0].clone()))))
        .method::<T>("addClass", &[o()], Some(me()), |t, _, a| opt(t.add_class(a[0].clone())))
        .method::<T>("attr", &[o(), o()], Some(me()), |t, _, a| opt(t.attr(a[0].clone(), a[1].clone())))
        .method::<T>("body", &[o()], Some(me()), |t, _, a| opt(Some(t.body(a[0].clone()))))
        .method::<T>("append", &[o()], Some(me()), |t, _, a| opt(Some(t.append(&a[0]))))
        .method::<T>("prepend", &[o()], Some(me()), |t, _, a| opt(Some(t.prepend(&a[0]))))
        .method::<T>("wrap", &[s()], Some(me()), |t, _, a| match a[0].as_str() {
            Some(d) => opt(t.wrap(d)),
            None => Err(super::npe()),
        })
        .method::<T>("orphan", &[], Some(me()), |t, _, _| opt(Some(t.orphan())))
        .method::<T>("parent", &[me()], Some(me()), |t, _, a| opt(Some(t.set_parent(as_tag(&a[0])))))
        .method::<T>("parent", &[], Some(me()), |t, _, _| opt(t.parent()))
        .method::<T>("root", &[], Some(me()), |t, _, _| opt(Some(t.root())))
        .method::<T>("children", &[], Some(c("java.util.List")), |t, _, _| val(t.children().map(|c| JValue::List(JList::new(ListKind::ArrayList, c))).unwrap_or(JValue::Null)))
        .method::<T>("isOrphan", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.is_orphan())))
        .method::<T>("isEmpty", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.is_empty())))
        .method::<T>("matches", &[me()], Some(JType::Boolean), |t, _, a| match as_tag(&a[0]) {
            Some(other) => val(JValue::Bool(t.matches(&other))),
            None => Err(super::npe()),
        })
        .method::<T>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.java_string())));
    super::object_methods::<T>(def).register()
}

tool_object!(Tag, tag_def);

fn def() -> Arc<ClassDef> {
    type M = MarkupTool;
    let def = ClassDef::new(CLASS)
        .extends(&[safe::SAFE_CONFIG])
        .implements(&["java.io.Serializable"])
        .method::<M>("setTab", &[s()], None, |t, _, a| {
            t.set_tab(a[0].as_str().unwrap_or("null"));
            Ok(None)
        })
        .method::<M>("getTab", &[], Some(s()), |t, _, _| val(JValue::str(t.get_tab())))
        .method::<M>("get", &[s()], Some(c(TAG)), |t, _, a| match a[0].as_str() {
            Some(d) => opt(t.tag(d)),
            None => Err(super::npe()),
        })
        .method::<M>("tag", &[s()], Some(c(TAG)), |t, _, a| match a[0].as_str() {
            Some(d) => opt(t.tag(d)),
            None => Err(super::npe()),
        });
    super::object_methods::<M>(safe::safe_config_methods::<M>(def)).register()
}

tool_object!(MarkupTool, def);

pub(crate) fn class() -> JavaClass {
    JavaClass::tool(def(), || MarkupTool::new().value()).annotated(Annotation::default_key("mark"))
}
