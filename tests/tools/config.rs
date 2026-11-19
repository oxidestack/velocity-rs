//! `org.apache.velocity.tools.test.whitebox.ConfigTests`, with its support classes (and the default-package
//! `tools` configuration class) as registered classes.

use std::sync::Arc;

use java_compat::lang::{JList, JObject, JValue, ListKind};
use java_compat::reflect::JType;
use java_compat::text::Locale;
use velocity::tools::classes::{Annotation, JavaClass, Member, modifier::*};
use velocity::tools::configuration::*;
use velocity::tools::{math_tool, number, resource_tool, scope};

use crate::base::{classpath, s};

const XML_PATH: &str = "tools.test.xml";
const PROPS_PATH: &str = "tools.test.properties";

const FAKE_TOOL: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$FakeTool";
const FACTORED_TOOL: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$FactoredTool";
const BAD_FACTORY: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$BadFactory";
const NULL_FACTORY: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$NullFactory";
const WRONG_PURPOSE_FACTORY: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$WrongPurposeFactory";
const GOOD_FACTORY: &str = "org.apache.velocity.tools.test.whitebox.ConfigTests$GoodFactory";

/// An instance of a fixture class.
struct Instance(&'static str);

impl JObject for Instance {
    fn class_name(&self) -> String {
        self.0.into()
    }

    fn invoke(&self, _: &str, _: &[JValue]) -> Option<Result<Option<JValue>, java_compat::lang::JError>> {
        None
    }

    fn to_java_string(&self) -> String {
        java_compat::reflect::identity_string(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn instance(class: &'static str) -> JValue {
    JValue::Object(Arc::new(Instance(class)))
}

/// `tools.getConfiguration()` (`src/test/java/tools.java`).
fn tools_configuration() -> FactoryConfiguration {
    let mut easy = EasyFactoryConfiguration::new(false);
    easy.number("version", JValue::Double(2.0));
    easy.toolbox("request").property("locale", Locale::us().value()).tool(None, resource_tool::CLASS).unwrap();
    easy.toolbox("application").tool(Some("calc"), math_tool::CLASS).unwrap().tool(None, number::CLASS).unwrap().property("locale", Locale::french().value());
    easy.factory
}

fn fixtures() {
    classpath();
    let statics = |name: &str, returns: &str| Member::new(name, PUBLIC | STATIC, &[], Some(JType::class(returns)));
    JavaClass::new("tools")
        .method(statics("getConfiguration", FACTORY_CONFIGURATION).invoker(|_| Ok(Some(tools_configuration().into_value()))))
        .constructor(Member::new("tools", PUBLIC, &[], None))
        .instantiable(|| Ok(instance("tools")))
        .register();
    JavaClass::new(FAKE_TOOL).modifiers(PUBLIC | STATIC).annotated(Annotation::default_key("test")).instantiable(|| Ok(instance(FAKE_TOOL))).register();
    JavaClass::new(FACTORED_TOOL).modifiers(PUBLIC | STATIC).annotated(Annotation::default_key("factored")).constructor(Member::new(FACTORED_TOOL, PUBLIC, &[JType::string()], None)).register();
    JavaClass::new(BAD_FACTORY).modifiers(PUBLIC | STATIC).method(Member::new("doNothing", PUBLIC | STATIC, &[], None).invoker(|_| Ok(None))).register();
    JavaClass::new(NULL_FACTORY).modifiers(PUBLIC | STATIC).method(statics("createFactoredTool", FACTORED_TOOL).invoker(|_| Ok(Some(JValue::Null)))).register();
    JavaClass::new(WRONG_PURPOSE_FACTORY).modifiers(PUBLIC | STATIC).method(statics("createFactoredTool", FAKE_TOOL).invoker(|_| Ok(Some(instance(FAKE_TOOL))))).register();
    JavaClass::new(GOOD_FACTORY).modifiers(PUBLIC | STATIC).method(statics("createFactoredTool", FACTORED_TOOL).invoker(|_| Ok(Some(instance(FACTORED_TOOL))))).register();
}

fn get_base_config() -> FactoryConfiguration {
    let mut base = FactoryConfiguration::new();
    let mut datum = Data::new();
    datum.set_key(Some("version"));
    datum.set_type(Some("number")).unwrap();
    datum.set_value(s("2.0"));
    base.add_data(datum);

    let mut toolbox = ToolboxConfiguration::new();
    toolbox.set_scope(scope::REQUEST);
    toolbox.set_property("locale", Locale::us().value());
    let mut tool = ToolConfiguration::new();
    tool.set_classname(Some(resource_tool::CLASS));
    toolbox.add_tool(tool);
    base.add_toolbox(toolbox);

    let mut toolbox = ToolboxConfiguration::new();
    toolbox.set_scope(scope::APPLICATION);
    let mut tool = ToolConfiguration::new();
    tool.set_key(Some("calc")).unwrap();
    tool.set_classname(Some(math_tool::CLASS));
    toolbox.add_tool(tool);
    let mut tool = ToolConfiguration::new();
    tool.set_classname(Some(number::CLASS));
    tool.set_property("locale", Locale::french().value());
    toolbox.add_tool(tool);
    base.add_toolbox(toolbox);
    base
}

#[track_caller]
fn assert_valid_factory(c: &FactoryConfiguration) {
    if let Err(e) = c.validate() {
        panic!("\n**** Unexpected Invalid Configuration ****\n{}\n{e}", c.java_string(true));
    }
}

#[track_caller]
fn assert_config_equals(one: &FactoryConfiguration, two: &FactoryConfiguration) {
    one.validate().unwrap();
    two.validate().unwrap();
    assert_eq!(one.java_string(false), two.java_string(false));
}

#[track_caller]
fn assert_valid_data(d: &Data) {
    if let Err(e) = d.validate() {
        panic!("\n**** Unexpected Invalid Data ****\n{}\n{e}", d.java_string());
    }
}

#[track_caller]
fn assert_invalid_data(d: &Data) {
    match d.validate() {
        Ok(()) => panic!("\n**** Unexpected Valid Data ****\n{}", d.java_string()),
        Err(e) => assert!(e.class.starts_with("org.apache.velocity.tools.config."), "{e}"),
    }
}

#[track_caller]
fn assert_invalid_tool(t: &ToolConfiguration) {
    if t.validate().is_ok() {
        panic!("\n**** Unexpected Valid Configuration ****\n{}", t.java_string());
    }
}

fn read_xml() -> FactoryConfiguration {
    let mut xml = FileFactoryConfiguration::xml("");
    xml.read(XML_PATH).unwrap();
    xml.factory
}

fn read_props() -> FactoryConfiguration {
    let mut props = FileFactoryConfiguration::properties("");
    props.read(PROPS_PATH).unwrap();
    props.factory
}

#[test]
fn test_base_config() {
    fixtures();
    assert_valid_factory(&get_base_config());
}

#[test]
fn test_xml_config() {
    fixtures();
    let xml = read_xml();
    assert_valid_factory(&xml);
    assert_config_equals(&get_base_config(), &xml);
}

#[test]
fn test_props_config() {
    fixtures();
    let props = read_props();
    assert_valid_factory(&props);
    assert_config_equals(&get_base_config(), &props);
}

#[test]
fn test_props_plus_xml_config() {
    fixtures();
    let props = read_props();
    let mut xml = read_xml();
    assert_config_equals(&props, &xml);
    xml.add_configuration(&props);
    assert_valid_factory(&xml);
    assert_config_equals(&xml, &props);
}

#[test]
fn test_easy_config() {
    fixtures();
    let easy = tools_configuration();
    assert_valid_factory(&easy);
    assert_config_equals(&get_base_config(), &easy);
}

#[test]
fn test_tools_class_config() {
    fixtures();
    let java = get_from_class("tools").unwrap();
    assert_valid_factory(&java);
    assert_config_equals(&get_base_config(), &java);
}

#[test]
fn test_default_config() {
    fixtures();
    assert_valid_factory(&get_default_tools().unwrap());
}

#[test]
fn test_auto_config() {
    fixtures();
    assert_valid_factory(&get_default_tools().unwrap());
}

#[test]
fn test_bad_data() {
    fixtures();
    let mut datum = Data::new();
    assert_invalid_data(&datum);
    datum.set_key(Some("test"));
    assert_invalid_data(&datum);
    datum.set_value(s("true"));
    datum.set_type(Some("number")).unwrap();
    assert_invalid_data(&datum);
    datum.set_value(s("0.1"));
    datum.convert_with(Converter::Integer);
    assert_invalid_data(&datum);
}

fn converted(d: &Data) -> JValue {
    d.get_converted_value().unwrap()
}

#[test]
fn test_data() {
    fixtures();
    let mut datum = Data::new();
    datum.set_key(Some("test"));
    datum.set_value(s("true"));
    assert_valid_data(&datum);
    assert_eq!(datum.get_type(), Some("auto"));
    assert!(matches!(converted(&datum), JValue::Bool(true)));
    datum.set_value(s("1"));
    assert!(matches!(converted(&datum), JValue::Int(1)));
    datum.set_value(s(&format!("{}0", i32::MAX)));
    assert_eq!(converted(&datum).class_name(), "java.lang.Long");
    datum.set_value(s("1.2"));
    assert!(matches!(converted(&datum), JValue::Double(d) if d == 1.2));
    datum.set_value(s("java.lang.Integer.MIN_VALUE"));
    assert!(matches!(converted(&datum), JValue::Int(i32::MIN)));
    datum.set_value(s("yes"));
    assert!(matches!(converted(&datum), JValue::Bool(true)));
    datum.set_type(Some("boolean")).unwrap();
    assert_valid_data(&datum);
    assert!(matches!(converted(&datum), JValue::Bool(true)));
    datum.set_value(s("java.lang.Boolean.TRUE"));
    datum.set_type(Some("field")).unwrap();
    assert_valid_data(&datum);
    assert!(matches!(converted(&datum), JValue::Bool(true)));
    datum.set_value(s("blahblah"));
    assert_invalid_data(&datum);
    datum.set_value(s("3.16"));
    datum.set_type(Some("number")).unwrap();
    assert_valid_data(&datum);
    assert!(matches!(converted(&datum), JValue::Double(d) if d == 3.16));
    datum.set_type(Some("string")).unwrap();
    assert_valid_data(&datum);
    assert_eq!(converted(&datum).as_str(), Some("3.16"));
    datum.set_type(Some("list")).unwrap();
    assert_valid_data(&datum);
    let list = |v: Vec<JValue>| JValue::List(JList::new(ListKind::ArrayList, v));
    assert!(java_compat::lang::java_equals(&converted(&datum), &list(vec![s("3.16")])));
    datum.set_value(s("1,2,3"));
    assert_valid_data(&datum);
    assert!(java_compat::lang::java_equals(&converted(&datum), &list(vec![s("1"), s("2"), s("3")])));
    datum.set_type(Some("list.number")).unwrap();
    assert_valid_data(&datum);
    assert!(java_compat::lang::java_equals(&converted(&datum), &list(vec![JValue::Int(1), JValue::Int(2), JValue::Int(3)])));
    datum.set_type(Some("list.boolean")).unwrap();
    datum.set_value(s("true,false"));
    let two = list(vec![JValue::Bool(true), JValue::Bool(false)]);
    assert!(java_compat::lang::java_equals(&converted(&datum), &two));
    datum.set_type(Some("list.field")).unwrap();
    datum.set_value(s("java.lang.Boolean.TRUE,java.lang.Boolean.FALSE"));
    assert!(java_compat::lang::java_equals(&converted(&datum), &two));
}

#[test]
fn test_configuration() {
    let mut conf = Configuration::new();
    conf.validate().unwrap();
    conf.set_property("string", s("whatever"));
    conf.validate().unwrap();
    assert_eq!(conf.get_property("string").unwrap().get_value().as_str(), Some("whatever"));
    conf.set_property("boolean", s("true"));
    conf.validate().unwrap();
    assert!(matches!(conf.get_property("boolean").unwrap().get_converted_value().unwrap(), JValue::Bool(true)));
    let this = instance("org.apache.velocity.tools.test.whitebox.ConfigTests");
    conf.set_property("testclass", this.clone());
    conf.validate().unwrap();
    assert!(java_compat::lang::java_equals(conf.get_property("testclass").unwrap().get_value(), &this));
    let mut cfg = Configuration::new();
    cfg.set_property("string", s("whoever"));
    conf.add_configuration(&cfg);
    assert_eq!(conf.get_property("string").unwrap().get_value().as_str(), Some("whoever"));
}

#[test]
fn test_bad_tool_config() {
    fixtures();
    let mut tool = ToolConfiguration::new();
    assert_invalid_tool(&tool);
    tool.set_classname(Some("no.such.Class"));
    assert_invalid_tool(&tool);
}

#[test]
fn test_tool_config() {
    fixtures();
    let mut tool = ToolConfiguration::new();
    tool.set_classname(Some(FAKE_TOOL));
    tool.validate().unwrap();
    assert_eq!(tool.get_key().unwrap().as_deref(), Some("test"));
    tool.set_key(Some("fake")).unwrap();
    assert_eq!(tool.get_key().unwrap().as_deref(), Some("fake"));
    let mut alt = ToolConfiguration::new();
    assert!(alt.get_classname().is_none());
    alt.add_configuration(&tool);
    assert_eq!(alt.get_classname(), Some(FAKE_TOOL));
    let alt = ToolConfiguration::new();
    assert!(alt.get_classname().is_none());
    tool.add_configuration(&alt);
    assert_eq!(tool.get_classname(), Some(FAKE_TOOL));
}

/// `FakeTool.foo()`: `@Test @Ignore`, an empty method that "exists only to keep the testrunner happy"; its
/// (empty) body runs here.
#[test]
fn fake_tool_foo() {}

#[test]
fn test_factories() {
    fixtures();
    let mut tool = ToolConfiguration::new();
    tool.set_classname(Some(FACTORED_TOOL));
    tool.set_factory_classname(Some(BAD_FACTORY));
    assert_invalid_tool(&tool);
    tool.set_factory_classname(Some(NULL_FACTORY));
    assert_invalid_tool(&tool);
    tool.set_factory_classname(Some(WRONG_PURPOSE_FACTORY));
    assert_invalid_tool(&tool);
    tool.set_factory_classname(Some(GOOD_FACTORY));
    tool.validate().unwrap();
}
