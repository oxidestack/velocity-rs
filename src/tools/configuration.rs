//! `org.apache.velocity.tools.config`: the configuration model a `ToolManager` is built from — data, properties,
//! tools, toolboxes, factories — and how it is read from Java-less sources (`EasyFactoryConfiguration`),
//! `.properties` files and XML files, cleaned and validated.

use std::sync::Arc;

use java_compat::lang::{JError, JList, JMap, JObject, JValue, MapKind};
use parking_lot::Mutex;

use super::classes::{self, JavaClass};
use super::toolbox::{TOOLKEY_KEY, ToolInfo};
use super::xml::java_compare;

/// The `ConfigurationException` class name: a configuration is invalid or cannot be read.
pub const CONFIGURATION_EXCEPTION: &str = "org.apache.velocity.tools.config.ConfigurationException";
/// The `NullKeyException` class name: a data item or tool has no key.
pub const NULL_KEY_EXCEPTION: &str = "org.apache.velocity.tools.config.NullKeyException";
/// The `InvalidScopeException` class name: a tool is configured in a scope it does not allow.
pub const INVALID_SCOPE_EXCEPTION: &str = "org.apache.velocity.tools.config.InvalidScopeException";

fn configuration_error(message: impl Into<String>) -> JError {
    JError::new(CONFIGURATION_EXCEPTION, message)
}

/// `new ConfigurationException(source, cause)`: the cause's `toString()` as the message.
fn configuration_cause(cause: &JError) -> JError {
    JError::new(CONFIGURATION_EXCEPTION, cause.to_string())
}

// ---------------------------------------------------------------------------------------------------------
// Data

/// `Data.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// `AUTO`: booleans, numbers and field paths recognised in the string, else the string.
    Auto,
    /// `BOOLEAN`: converted to a `Boolean`.
    Boolean,
    /// `CUSTOM`: the value as is, unless a converter is set.
    Custom,
    /// `FIELD`: the value of the static field the string names.
    Field,
    /// `NUMBER`: converted to an `Integer`, `Long` or `Double`.
    Number,
    /// `STRING`: converted to a `String`.
    String,
    /// `LIST`: a comma-separated list of strings.
    List,
    /// `LIST_AUTO`: a comma-separated list, each item converted as `AUTO`.
    ListAuto,
    /// `LIST_BOOLEAN`: a comma-separated list of `Boolean`s.
    ListBoolean,
    /// `LIST_FIELD`: a comma-separated list of static field values.
    ListField,
    /// `LIST_NUMBER`: a comma-separated list of numbers.
    ListNumber,
    /// `LIST_STRING`: a comma-separated list of strings.
    ListString,
}

impl DataType {
    /// `Type.get(type)`: `CUSTOM` for none; `IllegalArgumentException` for an unknown name.
    pub fn get(name: Option<&str>) -> Result<DataType, JError> {
        let Some(n) = name.filter(|n| !n.is_empty()) else { return Ok(DataType::Custom) };
        let constant = n.replace('.', "_").to_uppercase();
        Ok(match constant.as_str() {
            "AUTO" => DataType::Auto,
            "BOOLEAN" => DataType::Boolean,
            "CUSTOM" => DataType::Custom,
            "FIELD" => DataType::Field,
            "NUMBER" => DataType::Number,
            "STRING" => DataType::String,
            "LIST" => DataType::List,
            "LIST_AUTO" => DataType::ListAuto,
            "LIST_BOOLEAN" => DataType::ListBoolean,
            "LIST_FIELD" => DataType::ListField,
            "LIST_NUMBER" => DataType::ListNumber,
            "LIST_STRING" => DataType::ListString,
            other => return Err(JError::new("java.lang.IllegalArgumentException", format!("No enum constant org.apache.velocity.tools.config.Data.Type.{other}"))),
        })
    }

    /// `value()`: `list.number`, `auto`, ….
    pub fn value(self) -> &'static str {
        match self {
            DataType::Auto => "auto",
            DataType::Boolean => "boolean",
            DataType::Custom => "custom",
            DataType::Field => "field",
            DataType::Number => "number",
            DataType::String => "string",
            DataType::List => "list",
            DataType::ListAuto => "list.auto",
            DataType::ListBoolean => "list.boolean",
            DataType::ListField => "list.field",
            DataType::ListNumber => "list.number",
            DataType::ListString => "list.string",
        }
    }

    fn is_list(self) -> bool {
        matches!(self, DataType::List | DataType::ListAuto | DataType::ListBoolean | DataType::ListField | DataType::ListNumber | DataType::ListString)
    }

    fn target(self) -> Option<&'static str> {
        Some(match self {
            DataType::Custom => return None,
            DataType::Boolean | DataType::ListBoolean => "java.lang.Boolean",
            DataType::Number | DataType::ListNumber => "java.lang.Number",
            DataType::String | DataType::ListString => "java.lang.String",
            _ => "java.lang.Object",
        })
    }

    fn converter(self) -> Option<Converter> {
        Some(match self {
            DataType::Auto | DataType::ListAuto => Converter::Auto,
            DataType::Boolean | DataType::ListBoolean => Converter::Boolean,
            DataType::Field | DataType::ListField => Converter::Field,
            DataType::Number | DataType::ListNumber => Converter::Number,
            DataType::String | DataType::ListString => Converter::String,
            DataType::Custom | DataType::List => return None,
        })
    }
}

/// A converter given as code: `convert(type, value)`.
pub type CustomConverter = Arc<dyn Fn(Option<&str>, &JValue) -> Result<JValue, JError> + Send + Sync>;

/// A commons-beanutils `Converter` (or one of `Data`'s own).
#[derive(Clone)]
pub enum Converter {
    /// `Data.AutoConverter`: booleans, numbers and field paths recognised in strings.
    Auto,
    /// beanutils `BooleanConverter`.
    Boolean,
    /// `Data.FieldConverter`: a static field's value.
    Field,
    /// `Data.NumberConverter`: an `Integer` or `Long` without a decimal point, else as parsed.
    Number,
    /// beanutils `StringConverter`.
    String,
    /// beanutils `IntegerConverter` / `LongConverter` / `DoubleConverter` (no default value).
    Integer,
    /// beanutils `LongConverter`.
    Long,
    /// beanutils `DoubleConverter`.
    Double,
    /// A converter given as code.
    Custom(CustomConverter),
}

impl std::fmt::Debug for Converter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Converter::Auto => "AutoConverter",
            Converter::Boolean => "BooleanConverter",
            Converter::Field => "FieldConverter",
            Converter::Number => "NumberConverter",
            Converter::String => "StringConverter",
            Converter::Integer => "IntegerConverter",
            Converter::Long => "LongConverter",
            Converter::Double => "DoubleConverter",
            Converter::Custom(_) => "Converter",
        })
    }
}

fn conversion_error(message: String) -> JError {
    JError::new("org.apache.commons.beanutils.ConversionException", message)
}

/// beanutils `AbstractConverter.convertArray`: an array's first element.
fn first_of_array(v: &JValue) -> JValue {
    match v {
        JValue::Array(a) => a.get(0).unwrap_or(JValue::Null),
        other => other.clone(),
    }
}

fn simple_class(target: Option<&str>) -> String {
    target.map(|t| t.rsplit('.').next().unwrap_or(t).to_string()).unwrap_or_else(|| "null".into())
}

impl Converter {
    /// `convert(type, value)`.
    pub fn convert(&self, target: Option<&str>, value: &JValue) -> Result<JValue, JError> {
        match self {
            Converter::Auto => Ok(match value {
                JValue::Str(s) => auto_convert(s).unwrap_or_else(|_| value.clone()),
                other => other.clone(),
            }),
            Converter::Field => {
                let path = value.as_str().ok_or_else(|| JError::new("java.lang.ClassCastException", format!("class {} cannot be cast to class java.lang.String", value.class_name())))?;
                classes::field_value(path).map_err(|e| JError::new("java.lang.IllegalArgumentException", format!("Could not retrieve value for field at {path}: {e}")))
            }
            Converter::Number => number_convert(value),
            Converter::Custom(f) => f(target, value),
            beanutils => beanutils.bean_convert(target, value),
        }
    }

    fn bean_convert(&self, target: Option<&str>, value: &JValue) -> Result<JValue, JError> {
        let v = first_of_array(value);
        let default_target = match self {
            Converter::Boolean => "java.lang.Boolean",
            Converter::String => "java.lang.String",
            Converter::Integer => "java.lang.Integer",
            Converter::Long => "java.lang.Long",
            _ => "java.lang.Double",
        };
        let target = target.unwrap_or(default_target);
        let name = format!("{:?}", self);
        if v.is_null() {
            return Err(conversion_error(format!("No value specified for '{}'", simple_class(Some(target)))));
        }
        if target == "java.lang.String" {
            return Ok(JValue::str(v.to_java_string()));
        }
        if v.class_name() == target {
            return Ok(v);
        }
        let fail = || conversion_error(format!("Error converting from '{}' to '{}' {}", simple_class(Some(&v.class_name())), simple_class(Some(target)), name));
        match self {
            Converter::Boolean => {
                if target != "java.lang.Boolean" {
                    return Err(fail());
                }
                match v.to_java_string().to_lowercase().as_str() {
                    "true" | "yes" | "y" | "on" | "1" => Ok(JValue::Bool(true)),
                    "false" | "no" | "n" | "off" | "0" => Ok(JValue::Bool(false)),
                    _ => Err(fail()),
                }
            }
            Converter::Integer | Converter::Long | Converter::Double => {
                if super::num::is_number(&v) {
                    return match target {
                        "java.lang.Integer" => Ok(JValue::Int(super::num::int_value(&v))),
                        "java.lang.Long" => Ok(JValue::Long(super::num::long_value(&v))),
                        "java.lang.Double" => Ok(JValue::Double(super::num::double_value(&v))),
                        _ => Err(fail()),
                    };
                }
                let s = v.to_java_string();
                let s = s.trim();
                if s.is_empty() {
                    return Err(conversion_error(format!("No value specified for '{}'", simple_class(Some(target)))));
                }
                let bad = |e: JError| conversion_error(format!("Error converting from 'String' to '{}' {}", simple_class(Some(target)), e.message.unwrap_or_default()));
                match target {
                    "java.lang.Integer" => crate::uberspect::parse_integral(s, 32).map(|i| JValue::Int(i as i32)).map_err(bad),
                    "java.lang.Long" => crate::uberspect::parse_integral(s, 64).map(JValue::Long).map_err(bad),
                    "java.lang.Double" => crate::uberspect::parse_double(s).map(JValue::Double).map_err(bad),
                    _ => Err(conversion_error(format!("{name} cannot handle conversion from 'String' to '{}'", simple_class(Some(target))))),
                }
            }
            _ => Err(fail()),
        }
    }
}

fn full_match(pattern: &str, s: &str) -> bool {
    java_compat::methods::java_regex(&format!("^(?:{pattern})$")).map(|r| r.is_match(s).unwrap_or(false)).unwrap_or(false)
}

/// `AutoConverter.convert(String)`.
fn auto_convert(value: &str) -> Result<JValue, JError> {
    if full_match("true|false|yes|no|y|n|on|off", value) {
        return Converter::Boolean.convert(Some("java.lang.Boolean"), &JValue::str(value));
    }
    if full_match("-?[0-9]+(\\.[0-9]+)?", value) {
        return number_convert(&JValue::str(value));
    }
    if full_match("(\\w+\\.)+\\w+", value) {
        return Converter::Field.convert(Some("java.lang.Object"), &JValue::str(value));
    }
    Ok(JValue::str(value))
}

/// `Data.NumberConverter`: parsed in `Locale.US`; without a decimal point, an `Integer` (or a `Long` beyond).
fn number_convert(obj: &JValue) -> Result<JValue, JError> {
    let n = super::conversion::to_number_in(obj, "default", &super::locale("en", "US", ""))
        .filter(|_| !obj.is_null())
        .ok_or_else(|| JError::new("java.lang.IllegalArgumentException", format!("Could not convert {} to a number", obj.to_java_string())))?;
    if obj.to_java_string().contains('.') {
        return Ok(n);
    }
    let d = super::num::double_value(&n);
    Ok(if d > i32::MAX as f64 || d < i32::MIN as f64 { JValue::Long(super::num::long_value(&n)) } else { JValue::Int(super::num::int_value(&n)) })
}

/// `org.apache.velocity.tools.config.Data`: a keyed value with a type and a conversion.
#[derive(Clone, Debug)]
pub struct Data {
    key: Option<String>,
    type_value: Option<String>,
    value: JValue,
    is_list: bool,
    target: Option<String>,
    converter: Option<Converter>,
}

impl Default for Data {
    fn default() -> Data {
        let mut d = Data { key: None, type_value: None, value: JValue::Null, is_list: false, target: None, converter: None };
        d.set_type_enum(DataType::Auto);
        d
    }
}

impl Data {
    /// `new Data()`: no key or value, of type `auto`.
    pub fn new() -> Data {
        Data::default()
    }

    /// `setKey(key)`.
    pub fn set_key(&mut self, key: Option<&str>) {
        self.key = key.map(str::to_string);
    }

    /// `setValue(value)`: the unconverted value.
    pub fn set_value(&mut self, value: JValue) {
        self.value = value;
    }

    /// `setClassname(name)`: the conversion's target class.
    pub fn set_classname(&mut self, name: &str) -> Result<(), JError> {
        match classes::for_name(name) {
            Some(_) => {
                self.target = Some(name.to_string());
                Ok(())
            }
            None => Err(JError::new("java.lang.IllegalArgumentException", format!("Class {name} could not be found."))),
        }
    }

    /// `setType(Type)`: the type's target class and converter, and whether the value is a list; `CUSTOM` leaves the type name, target and converter as they are.
    pub fn set_type_enum(&mut self, t: DataType) {
        self.is_list = t.is_list();
        if t != DataType::Custom {
            self.type_value = Some(t.value().to_string());
            self.target = t.target().map(str::to_string);
            self.converter = t.converter();
        }
    }

    /// `setType(String)`.
    pub fn set_type(&mut self, t: Option<&str>) -> Result<(), JError> {
        self.type_value = t.map(str::to_string);
        let ty = DataType::get(t)?;
        self.set_type_enum(ty);
        Ok(())
    }

    /// `setTargetClass(class)`: the class name the converter converts to, `None` for none.
    pub fn set_target_class(&mut self, class: Option<&str>) {
        self.target = class.map(str::to_string);
    }

    /// `convertWith(converter)`: the converter `getConvertedValue()` applies.
    pub fn convert_with(&mut self, converter: Converter) {
        self.converter = Some(converter);
    }

    /// `getKey()`: `None` until a key is set.
    pub fn get_key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// `getType()`: the type name (`auto`, `list.number`, …), `None` when unset.
    pub fn get_type(&self) -> Option<&str> {
        self.type_value.as_deref()
    }

    /// `getValue()`: the unconverted value.
    pub fn get_value(&self) -> &JValue {
        &self.value
    }

    /// `getTargetClass()`: the class name values are converted to, `None` for none.
    pub fn get_target_class(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// `getConverter()`: `None` when the value is used as is.
    pub fn get_converter(&self) -> Option<&Converter> {
        self.converter.as_ref()
    }

    /// `getConvertedValue()`.
    pub fn get_converted_value(&self) -> Result<JValue, JError> {
        if self.is_list {
            return self.convert_list();
        }
        match &self.converter {
            None => Ok(self.value.clone()),
            Some(c) => c.convert(self.target.as_deref(), &self.value),
        }
    }

    fn convert_list(&self) -> Result<JValue, JError> {
        let value = match &self.value {
            JValue::Null => return Ok(JValue::Null),
            JValue::Str(s) => s.to_string(),
            other => return Err(JError::new("java.lang.ClassCastException", format!("class {} cannot be cast to class java.lang.String", other.class_name()))),
        };
        if value.trim().is_empty() {
            return Ok(JValue::Null);
        }
        let items = java_compat::methods::split(&value, ",", 0)?;
        match &self.converter {
            Some(c) if self.target.as_deref() != Some("java.lang.String") => {
                let converted = items.iter().map(|i| c.convert(self.target.as_deref(), &JValue::str(i.as_str()))).collect::<Result<Vec<_>, _>>()?;
                Ok(JValue::List(JList::array_list(converted)))
            }
            _ => Ok(JValue::List(JList::new(java_compat::lang::ListKind::Fixed, items.into_iter().map(JValue::str).collect()))),
        }
    }

    /// `validate()`: a key, a value, and a conversion that succeeds (and does not give `null`).
    pub fn validate(&self) -> Result<(), JError> {
        if self.key.is_none() {
            return Err(JError::new(NULL_KEY_EXCEPTION, format!("Key is null for data with value of '{}'", self.value.to_java_string())));
        }
        if self.value.is_null() {
            return Err(configuration_error(format!("No value has been set for '{}'", self.key.as_deref().unwrap_or("null"))));
        }
        if self.converter.is_some() {
            match self.get_converted_value() {
                Ok(JValue::Null) => {
                    return Err(configuration_error(format!("Conversion of {} for '{}' failed and returned null", self.value.to_java_string(), self.key.as_deref().unwrap_or("null"))));
                }
                Ok(_) => {}
                Err(e) => return Err(configuration_cause(&e)),
            }
        }
        Ok(())
    }

    /// `compareTo`: by key, `null` first.
    fn compare(&self, other: &Data) -> std::cmp::Ordering {
        match (&self.key, &other.key) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, _) => std::cmp::Ordering::Less,
            (_, None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => java_compare(a, b),
        }
    }

    /// `toString()`: `Data 'key' -type-> value`.
    pub fn java_string(&self) -> String {
        format!("Data '{}' -{}-> {}", self.key.as_deref().unwrap_or("null"), self.type_value.as_deref().unwrap_or("null"), self.value.to_java_string())
    }

    /// `BeanUtils.populate(data, props)`: the writable properties among the keys.
    fn populate(&mut self, props: &crate::props::ExtProperties) -> Result<(), JError> {
        for key in props.keys() {
            let value = props.string(key);
            match key.as_str() {
                "key" => self.set_key(value.as_deref()),
                "value" => self.set_value(value.map(JValue::str).unwrap_or(JValue::Null)),
                "type" => self.set_type(value.as_deref())?,
                "class" | "classname" => {
                    if let Some(v) = value {
                        self.set_classname(&v)?;
                    }
                }
                "targetClass" => self.set_target_class(value.as_deref()),
                _ => {}
            }
        }
        Ok(())
    }
}

/// `Property`: a `Data` named by its key.
pub type Property = Data;

impl Data {
    /// `Property.setName(name)`.
    pub fn named(name: &str, value: JValue) -> Property {
        let mut p = Data::new();
        p.set_key(Some(name));
        p.set_value(value);
        p
    }

    /// `Property.getName()`: the key.
    pub fn get_name(&self) -> Option<&str> {
        self.get_key()
    }
}

// ---------------------------------------------------------------------------------------------------------
// Configuration

/// `Configuration`: properties sorted by name.
#[derive(Clone, Debug, Default)]
pub struct Configuration {
    properties: Vec<Property>,
}

impl Configuration {
    /// An empty configuration.
    pub fn new() -> Configuration {
        Configuration::default()
    }

    /// `addProperty(property)`: replaces one of the same name.
    pub fn add_property(&mut self, p: Property) -> Result<(), JError> {
        if p.get_name().is_none() {
            return Err(JError::new("java.lang.IllegalArgumentException", "All properties must be named before they can be added to the configuration."));
        }
        self.properties.retain(|x| x.get_name() != p.get_name());
        let at = self.properties.iter().position(|x| x.compare(&p) == std::cmp::Ordering::Greater).unwrap_or(self.properties.len());
        self.properties.insert(at, p);
        Ok(())
    }

    /// `setProperty(name, value)`: adds or replaces the property `name`.
    pub fn set_property(&mut self, name: &str, value: JValue) {
        self.add_property(Data::named(name, value)).expect("a named property");
    }

    /// `removeProperty(name)`: whether a property was removed.
    pub fn remove_property(&mut self, name: &str) -> bool {
        let n = self.properties.len();
        self.properties.retain(|p| p.get_name() != Some(name));
        n != self.properties.len()
    }

    /// `hasProperties()`.
    pub fn has_properties(&self) -> bool {
        !self.properties.is_empty()
    }

    /// `getProperty(name)`: `None` when there is no such property.
    pub fn get_property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.get_name() == Some(name))
    }

    /// `getProperties()`: a copy of the properties, sorted by name.
    pub fn get_properties(&self) -> Vec<Property> {
        self.properties.clone()
    }

    /// `getPropertyMap()`: the converted values in a `HashMap`.
    pub fn get_property_map(&self) -> Result<JMap, JError> {
        let map = JMap::new(MapKind::Hash);
        for p in &self.properties {
            map.put_str(p.get_name().unwrap_or("null"), p.get_converted_value()?);
        }
        Ok(map)
    }

    /// `setPropertyMap(map)`: a property for each entry, named by the key's string form.
    pub fn set_property_map(&mut self, props: &JMap) {
        for (k, v) in props.entries() {
            self.set_property(&k.to_java_string(), v);
        }
    }

    /// `setProperties(properties)`: adds each; `IllegalArgumentException` for an unnamed one.
    pub fn set_properties(&mut self, props: Vec<Property>) -> Result<(), JError> {
        for p in props {
            self.add_property(p)?;
        }
        Ok(())
    }

    /// `addConfiguration(config)`: the other's properties added, replacing those of the same name.
    pub fn add_configuration(&mut self, config: &Configuration) {
        let _ = self.set_properties(config.get_properties());
    }

    /// `validate()`: each property validated, the first failure returned.
    pub fn validate(&self) -> Result<(), JError> {
        for p in &self.properties {
            p.validate()?;
        }
        Ok(())
    }

    fn append_properties(&self, out: &mut String) {
        if self.has_properties() {
            out.push_str(&format!("with {} properties [", self.properties.len()));
            for p in &self.properties {
                out.push_str(&format!("{} -{}-> {}; ", p.get_key().unwrap_or("null"), p.get_type().unwrap_or("null"), p.get_value().to_java_string()));
            }
            out.push(']');
        }
    }

    /// `ConfigurationCleaner.cleanProperties`: iterates a copy, so nothing is actually removed.
    fn clean_properties(&self) {
        for p in self.get_properties() {
            if let Err(e) = p.validate() {
                super::log::debug("org.apache.velocity.tools.config.ConfigurationCleaner", &format!("ConfigurationCleaner : {e}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// ToolConfiguration

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Valid,
    None,
    Missing,
    #[allow(dead_code)]
    Unsupported,
    Uninstantiable,
}

/// `ToolConfiguration`: a tool's class (or factory), key, path restriction and properties.
#[derive(Debug, Default)]
pub struct ToolConfiguration {
    /// The tool's properties.
    pub config: Configuration,
    key: Option<String>,
    classname: Option<String>,
    factory_classname: Option<String>,
    restrict_to: Option<String>,
    skip_setters: Option<bool>,
    status: Mutex<Option<(Status, Option<JError>)>>,
}

impl Clone for ToolConfiguration {
    fn clone(&self) -> ToolConfiguration {
        ToolConfiguration {
            config: self.config.clone(),
            key: self.key.clone(),
            classname: self.classname.clone(),
            factory_classname: self.factory_classname.clone(),
            restrict_to: self.restrict_to.clone(),
            skip_setters: self.skip_setters,
            status: Mutex::new(self.status.lock().clone()),
        }
    }
}

impl ToolConfiguration {
    /// A tool configuration with no class, key or properties.
    pub fn new() -> ToolConfiguration {
        ToolConfiguration::default()
    }

    /// `setKey(key)`: also a `key` property when it is not the default key.
    pub fn set_key(&mut self, key: Option<&str>) -> Result<(), JError> {
        self.key = key.map(str::to_string);
        if let Some(k) = key {
            if Some(k.to_string()) != self.get_default_key()? {
                self.config.set_property(TOOLKEY_KEY, JValue::str(k));
            }
        }
        Ok(())
    }

    /// `setClassname(name)`: the tool's class name; the tool is checked again on next use.
    pub fn set_classname(&mut self, classname: Option<&str>) {
        self.classname = classname.map(str::to_string);
        *self.status.lock() = None;
    }

    /// `setFactoryClassname(name)`: the class whose static factory method creates the tool.
    pub fn set_factory_classname(&mut self, name: Option<&str>) {
        self.factory_classname = name.map(str::to_string);
        *self.status.lock() = None;
    }

    /// `setRestrictTo(path)`: the request paths the tool is restricted to, `None` for all.
    pub fn set_restrict_to(&mut self, path: Option<&str>) {
        self.restrict_to = path.map(str::to_string);
    }

    /// `setSkipSetters(skip)`: whether configuration properties are not passed to the tool's setters; `None` leaves it to `@SkipSetters`.
    pub fn set_skip_setters(&mut self, skip: Option<bool>) {
        self.skip_setters = skip;
    }

    /// `setProperty(name, value)`: a tool property.
    pub fn set_property(&mut self, name: &str, value: JValue) {
        self.config.set_property(name, value);
    }

    /// `getKey()`: the key set, else the default key.
    pub fn get_key(&self) -> Result<Option<String>, JError> {
        if self.key.is_some() {
            return Ok(self.key.clone());
        }
        self.get_default_key()
    }

    /// `getDefaultKey()`: `@DefaultKey`, else the simple class name without `Tool`, uncapitalized.
    pub fn get_default_key(&self) -> Result<Option<String>, JError> {
        if self.classname.is_none() {
            return Ok(None);
        }
        let class = self.get_tool_class()?;
        if let Some(a) = class.annotation(classes::DEFAULT_KEY) {
            if let Some(classes::AnnotationValue::Str(v)) = a.value("value") {
                return Ok(Some(v.clone()));
            }
        }
        let mut name = class.simple_name();
        if name.ends_with("Tool") {
            let i = name.find("Tool").expect("a Tool suffix");
            name.truncate(i);
        }
        let units: Vec<u16> = name.encode_utf16().collect();
        Ok(Some(if units.len() > 1 { String::from_utf16_lossy(&units[..1]).to_lowercase() + &String::from_utf16_lossy(&units[1..]) } else { name.to_lowercase() }))
    }

    /// `getClassname()`.
    pub fn get_classname(&self) -> Option<&str> {
        self.classname.as_deref()
    }

    /// `getFactoryClassname()`.
    pub fn get_factory_classname(&self) -> Option<&str> {
        self.factory_classname.as_deref()
    }

    /// `getToolClass()`: `ConfigurationException` when the class is not found.
    pub fn get_tool_class(&self) -> Result<Arc<JavaClass>, JError> {
        classes::get_class(self.classname.as_deref().unwrap_or("null")).map_err(|e| configuration_cause(&e))
    }

    /// `getFactoryClass()`: `None` without a factory, `ConfigurationException` when its class is not found.
    pub fn get_factory(&self) -> Result<Option<Arc<JavaClass>>, JError> {
        match &self.factory_classname {
            None => Ok(None),
            Some(n) => classes::get_class(n).map(Some).map_err(|e| configuration_cause(&e)),
        }
    }

    fn scopes(&self, annotation: &str) -> Result<Vec<String>, JError> {
        let class = self.get_tool_class()?;
        Ok(match class.annotation(annotation).and_then(|a| a.value("value").cloned()) {
            Some(classes::AnnotationValue::Strings(v)) => v,
            _ => Vec::new(),
        })
    }

    /// The scopes the tool's `@InvalidScope` names, empty without it; `ConfigurationException` when the tool class is not found.
    pub fn get_invalid_scopes(&self) -> Result<Vec<String>, JError> {
        self.scopes(classes::INVALID_SCOPE)
    }

    /// The scopes the tool's `@ValidScope` names, empty without it; `ConfigurationException` when the tool class is not found.
    pub fn get_valid_scopes(&self) -> Result<Vec<String>, JError> {
        self.scopes(classes::VALID_SCOPE)
    }

    fn status(&self) -> (Status, Option<JError>) {
        if let Some(s) = self.status.lock().clone() {
            return s;
        }
        let s = match &self.classname {
            None => (Status::None, None),
            Some(name) => match classes::get_class(name) {
                Err(e) => (Status::Missing, Some(e)),
                Ok(class) => match self.check_instantiable(&class) {
                    Ok(()) => (Status::Valid, None),
                    Err(e) => (Status::Uninstantiable, Some(e)),
                },
            },
        };
        *self.status.lock() = Some(s.clone());
        s
    }

    fn check_instantiable(&self, class: &JavaClass) -> Result<(), JError> {
        match &self.factory_classname {
            None => class.new_instance().map(|_| ()),
            Some(f) => {
                let factory = classes::get_class(f)?;
                let simple = class.simple_name();
                let Some(method) = classes::find_factory_method(&factory, class) else {
                    return Err(JError::new("java.lang.IllegalArgumentException", format!("Factory class hasn't any method named create{simple}(), new{simple}() or get{simple}()")));
                };
                let signature = format!("public static {} {}.{}()", method.returns.as_ref().map(|t| t.type_name()).unwrap_or_else(|| "void".into()), factory.name, method.name);
                let invoke = method.invoke.clone().ok_or_else(|| JError::new("java.lang.NullPointerException", "Cannot invoke instance method without an instance"))?;
                let instance = invoke(&[])?.unwrap_or(JValue::Null);
                if instance.is_null() {
                    return Err(JError::new("java.lang.invoke.WrongMethodTypeException", format!("Factory method '{signature}' returned null")));
                }
                let instance_class = instance.class_name();
                if !class.is_assignable_from(&instance_class) {
                    return Err(JError::new("java.lang.invoke.WrongMethodTypeException", format!("Factory method '{signature}' is expected to return an instance of class '{}'", class.name)));
                }
                Ok(())
            }
        }
    }

    /// `getRestrictTo()`.
    pub fn get_restrict_to(&self) -> Option<&str> {
        self.restrict_to.as_deref()
    }

    /// `getSkipSetters()`: `None` when not configured.
    pub fn get_skip_setters(&self) -> Option<bool> {
        self.skip_setters
    }

    fn error(&self, status: Status, problem: &Option<JError>) -> String {
        let me = self.java_string();
        let p = problem.as_ref().map(|p| p.to_string()).unwrap_or_else(|| "null".into());
        match status {
            Status::None => format!("No classname set for: {me}"),
            Status::Missing => format!("Couldn't find tool class in the classpath for: {me}({p})"),
            Status::Unsupported => format!("Couldn't find necessary supporting classes for: {me}({p})"),
            Status::Uninstantiable => format!("Couldn't instantiate instance of tool for: {me}({p})"),
            Status::Valid => String::new(),
        }
    }

    /// `createInfo()`.
    pub fn create_info(&self) -> Result<ToolInfo, JError> {
        let (status, problem) = self.status();
        if status != Status::Valid {
            return Err(configuration_error(self.error(status, &problem)));
        }
        let key = self.get_key()?.ok_or_else(|| JError::new("java.lang.NullPointerException", "Key cannot be null"))?;
        let mut info = ToolInfo::new(&key, self.get_tool_class()?, self.get_factory()?);
        info.restrict_to(self.restrict_to.as_deref());
        if let Some(s) = self.skip_setters {
            info.set_skip_setters(s);
        }
        let properties = self.config.get_property_map()?;
        properties.put_str(TOOLKEY_KEY, JValue::str(key));
        info.add_properties(&properties);
        Ok(info)
    }

    /// `addConfiguration(config)`: properties, and a class and restriction when the other has one.
    pub fn add_configuration(&mut self, that: &ToolConfiguration) {
        self.config.add_configuration(&that.config);
        if let Some(c) = &that.classname {
            self.set_classname(Some(c));
        }
        if let Some(r) = &that.restrict_to {
            self.set_restrict_to(Some(r));
        }
    }

    /// `validate()`.
    pub fn validate(&self) -> Result<(), JError> {
        self.config.validate()?;
        if self.get_key()?.is_none() {
            return Err(JError::new(NULL_KEY_EXCEPTION, format!("Key is null for tool whose class is '{}'", self.classname.as_deref().unwrap_or("null"))));
        }
        let (status, problem) = self.status();
        if status != Status::Valid {
            return Err(configuration_error(self.error(status, &problem)));
        }
        Ok(())
    }

    fn sort_key(&self) -> Option<String> {
        self.get_key().ok().flatten()
    }

    /// `toString()`: the status, key, class, restriction and properties.
    pub fn java_string(&self) -> String {
        let mut out = String::new();
        if self.classname.is_none() {
            out.push_str("Tool '");
            out.push_str(self.key.as_deref().unwrap_or("null"));
        } else {
            let status = self.status.lock().clone();
            match status {
                None => out.push_str("Unchecked "),
                Some((s, _)) => out.push_str(match s {
                    Status::Valid => "",
                    Status::None | Status::Missing => "Invalid ",
                    Status::Unsupported => "Unsupported ",
                    Status::Uninstantiable => "Unusable ",
                }),
            }
            out.push_str("Tool '");
            out.push_str(&self.get_key().ok().flatten().unwrap_or_else(|| "null".into()));
        }
        out.push_str("' => ");
        out.push_str(self.classname.as_deref().unwrap_or("null"));
        if let Some(r) = &self.restrict_to {
            out.push_str(&format!(" only for '{r}'"));
        }
        out.push(' ');
        self.config.append_properties(&mut out);
        out
    }
}

// ---------------------------------------------------------------------------------------------------------
// ToolboxConfiguration

/// `ToolboxConfiguration`: a scope's tools (sorted by key) and properties (including `scope` itself).
#[derive(Clone, Debug)]
pub struct ToolboxConfiguration {
    /// The toolbox's properties, `scope` among them.
    pub config: Configuration,
    scope: String,
    tools: Vec<ToolConfiguration>,
}

impl Default for ToolboxConfiguration {
    fn default() -> ToolboxConfiguration {
        let mut t = ToolboxConfiguration { config: Configuration::new(), scope: super::toolbox::DEFAULT_SCOPE.into(), tools: Vec::new() };
        t.config.set_property("scope", JValue::str(super::toolbox::DEFAULT_SCOPE));
        t
    }
}

impl ToolboxConfiguration {
    /// A toolbox of the default `request` scope, with no tools.
    pub fn new() -> ToolboxConfiguration {
        ToolboxConfiguration::default()
    }

    /// `setScope(scope)`: also the `scope` property.
    pub fn set_scope(&mut self, scope: &str) {
        self.scope = scope.to_string();
        self.config.set_property("scope", JValue::str(scope));
    }

    /// `getScope()`.
    pub fn get_scope(&self) -> &str {
        &self.scope
    }

    /// `setProperty(name, value)`: a toolbox property.
    pub fn set_property(&mut self, name: &str, value: JValue) {
        self.config.set_property(name, value);
    }

    /// `addTool(tool)`: merged into one of the same key, else added in key order.
    pub fn add_tool(&mut self, tool: ToolConfiguration) {
        let key = tool.sort_key();
        if key.is_some() {
            if let Some(existing) = self.tools.iter_mut().find(|t| t.sort_key() == key) {
                existing.add_configuration(&tool);
                return;
            }
        }
        let cmp = |a: &Option<String>, b: &Option<String>| match (a, b) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, _) => std::cmp::Ordering::Less,
            (_, None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => java_compare(x, y),
        };
        if key.is_none() && self.tools.iter().any(|t| t.sort_key().is_none()) {
            return;
        }
        let at = self.tools.iter().position(|t| cmp(&t.sort_key(), &key) == std::cmp::Ordering::Greater).unwrap_or(self.tools.len());
        self.tools.insert(at, tool);
    }

    /// `removeTool(tool)`: the tool of this key, if any, removed.
    pub fn remove_tool(&mut self, key: &str) {
        self.tools.retain(|t| t.sort_key().as_deref() != Some(key));
    }

    /// `getTool(key)`: `None` when there is no tool of this key.
    pub fn get_tool(&self, key: &str) -> Option<&ToolConfiguration> {
        self.tools.iter().find(|t| t.sort_key().as_deref() == Some(key))
    }

    /// The tool of this key, mutably; `None` when there is none.
    pub fn get_tool_mut(&mut self, key: &str) -> Option<&mut ToolConfiguration> {
        self.tools.iter_mut().find(|t| t.sort_key().as_deref() == Some(key))
    }

    /// `getTools()`: sorted by key.
    pub fn get_tools(&self) -> &[ToolConfiguration] {
        &self.tools
    }

    /// The tools, mutably.
    pub fn tools_mut(&mut self) -> &mut Vec<ToolConfiguration> {
        &mut self.tools
    }

    /// `CompoundConfiguration.addConfiguration`: the other's tools, then its properties.
    pub fn add_configuration(&mut self, that: &ToolboxConfiguration) {
        for t in &that.tools {
            self.add_tool(t.clone());
        }
        self.config.add_configuration(&that.config);
    }

    /// `validate()`: properties, tools, a known scope, and each tool's valid / invalid scopes.
    pub fn validate(&self) -> Result<(), JError> {
        self.config.validate()?;
        for t in &self.tools {
            t.validate()?;
        }
        if !super::scope::exists(&self.scope) {
            return Err(configuration_error(format!(
                "Scope '{}' is not recognized. Please correct or add your new custom scope with org.apache.velocity.tools.Scope.add(\"{}\").",
                self.scope, self.scope
            )));
        }
        let invalid = |t: &ToolConfiguration| {
            JError::new(INVALID_SCOPE_EXCEPTION, format!("Toolbox with scope '{}' may not contain a {}.", self.scope, t.classname.as_deref().unwrap_or("null")))
        };
        for t in &self.tools {
            if t.get_invalid_scopes()?.contains(&self.scope) {
                return Err(invalid(t));
            }
            let valid = t.get_valid_scopes()?;
            if !valid.is_empty() && !valid.contains(&self.scope) {
                return Err(invalid(t));
            }
        }
        Ok(())
    }

    /// `toString()`: the scope, properties and tools.
    pub fn java_string(&self) -> String {
        let mut out = format!("Toolbox '{}' ", self.scope);
        self.config.append_properties(&mut out);
        append_children(&mut out, self.config.has_properties(), self.tools.iter().map(|t| t.java_string()).collect(), "tools: \n  ", "\n  ");
        out
    }
}

fn append_children(out: &mut String, has_properties: bool, children: Vec<String>, name: &str, delim: &str) {
    if children.is_empty() {
        return;
    }
    out.push_str(if has_properties { " and " } else { " with " });
    out.push_str(&children.len().to_string());
    out.push(' ');
    out.push_str(name);
    for c in children {
        out.push_str(&c);
        out.push_str(delim);
    }
}

// ---------------------------------------------------------------------------------------------------------
// FactoryConfiguration

/// `FactoryConfiguration`: data (sorted by key), toolboxes (sorted by scope), properties and the sources read.
#[derive(Clone, Debug)]
pub struct FactoryConfiguration {
    /// The factory's properties.
    pub config: Configuration,
    data: Vec<Data>,
    toolboxes: Vec<ToolboxConfiguration>,
    sources: Vec<String>,
}

impl Default for FactoryConfiguration {
    fn default() -> FactoryConfiguration {
        FactoryConfiguration::with_source("")
    }
}

/// The `FactoryConfiguration` class name, the prefix of a plain configuration's source.
pub const FACTORY_CONFIGURATION: &str = "org.apache.velocity.tools.config.FactoryConfiguration";

impl FactoryConfiguration {
    /// An empty configuration whose source is `FactoryConfiguration()`.
    pub fn new() -> FactoryConfiguration {
        FactoryConfiguration::default()
    }

    /// `new FactoryConfiguration(source)`.
    pub fn with_source(source: &str) -> FactoryConfiguration {
        FactoryConfiguration::for_class(FACTORY_CONFIGURATION, source)
    }

    /// `FactoryConfiguration(Class, String)`: the source is `class(source)`.
    pub fn for_class(class: &str, source: &str) -> FactoryConfiguration {
        FactoryConfiguration { config: Configuration::new(), data: Vec::new(), toolboxes: Vec::new(), sources: vec![format!("{class}({source})")] }
    }

    /// `getSource()`: the first source, the one this configuration was created with.
    pub fn get_source(&self) -> &str {
        &self.sources[0]
    }

    /// `setSource(source)`: replaces the first source.
    pub fn set_source(&mut self, s: &str) {
        self.sources[0] = s.to_string();
    }

    /// `getSources()`: the creating source, then those of each configuration added.
    pub fn get_sources(&self) -> &[String] {
        &self.sources
    }

    /// The sources, mutably.
    pub fn sources_mut(&mut self) -> &mut Vec<String> {
        &mut self.sources
    }

    /// `addSource(source)`.
    pub fn add_source(&mut self, s: &str) {
        self.sources.push(s.to_string());
    }

    /// `setProperty(name, value)`: a factory property.
    pub fn set_property(&mut self, name: &str, value: JValue) {
        self.config.set_property(name, value);
    }

    /// `addData(datum)`: replaces one of the same key.
    pub fn add_data(&mut self, d: Data) {
        if d.get_key().is_some() {
            self.data.retain(|x| x.get_key() != d.get_key());
        } else if self.data.iter().any(|x| x.get_key().is_none()) {
            return;
        }
        let at = self.data.iter().position(|x| x.compare(&d) == std::cmp::Ordering::Greater).unwrap_or(self.data.len());
        self.data.insert(at, d);
    }

    /// `removeData(data)`: the data item of this key removed; whether one was.
    pub fn remove_data(&mut self, key: Option<&str>) -> bool {
        let n = self.data.len();
        self.data.retain(|x| x.get_key() != key);
        n != self.data.len()
    }

    /// `getData(key)`: `None` when there is no data of this key.
    pub fn get_data(&self, key: &str) -> Option<&Data> {
        self.data.iter().find(|d| d.get_key() == Some(key))
    }

    /// `hasData()`.
    pub fn has_data(&self) -> bool {
        !self.data.is_empty()
    }

    /// `getData()`: sorted by key.
    pub fn data(&self) -> &[Data] {
        &self.data
    }

    /// The data, mutably.
    pub fn data_mut(&mut self) -> &mut Vec<Data> {
        &mut self.data
    }

    /// `addToolbox(toolbox)`: merged into one of the same scope, else added in scope order.
    pub fn add_toolbox(&mut self, toolbox: ToolboxConfiguration) {
        if let Some(existing) = self.toolboxes.iter_mut().find(|t| t.scope == toolbox.scope) {
            existing.add_configuration(&toolbox);
            return;
        }
        let at = self.toolboxes.iter().position(|t| java_compare(&t.scope, &toolbox.scope) == std::cmp::Ordering::Greater).unwrap_or(self.toolboxes.len());
        self.toolboxes.insert(at, toolbox);
    }

    /// `removeToolbox(toolbox)`: the toolbox of this scope, if any, removed.
    pub fn remove_toolbox(&mut self, scope: &str) {
        self.toolboxes.retain(|t| t.scope != scope);
    }

    /// `getToolbox(scope)`: `None` when there is no toolbox of this scope.
    pub fn get_toolbox(&self, scope: &str) -> Option<&ToolboxConfiguration> {
        self.toolboxes.iter().find(|t| t.scope == scope)
    }

    /// The toolbox of this scope, mutably; `None` when there is none.
    pub fn get_toolbox_mut(&mut self, scope: &str) -> Option<&mut ToolboxConfiguration> {
        self.toolboxes.iter_mut().find(|t| t.scope == scope)
    }

    /// `getToolboxes()`: sorted by scope.
    pub fn get_toolboxes(&self) -> &[ToolboxConfiguration] {
        &self.toolboxes
    }

    /// The toolboxes, mutably.
    pub fn toolboxes_mut(&mut self) -> &mut Vec<ToolboxConfiguration> {
        &mut self.toolboxes
    }

    /// `addConfiguration(config)`: data, sources, toolboxes, then properties.
    pub fn add_configuration(&mut self, that: &FactoryConfiguration) {
        for d in &that.data {
            self.add_data(d.clone());
        }
        for s in &that.sources {
            self.add_source(s);
        }
        for t in &that.toolboxes {
            self.add_toolbox(t.clone());
        }
        self.config.add_configuration(&that.config);
    }

    /// `validate()`.
    pub fn validate(&self) -> Result<(), JError> {
        self.config.validate()?;
        for t in &self.toolboxes {
            t.validate()?;
        }
        for d in &self.data {
            d.validate()?;
        }
        Ok(())
    }

    /// `toString(includeSources)`.
    pub fn java_string(&self, include_sources: bool) -> String {
        let mut out = String::from("\nFactoryConfiguration from ");
        if include_sources {
            out.push_str(&format!("{} sources ", self.sources.len()));
        }
        self.config.append_properties(&mut out);
        if self.has_data() {
            out.push_str(&format!("including {} data", self.data.len()));
        }
        if self.toolboxes.is_empty() {
            out.push_str("\n ");
        } else {
            append_children(&mut out, self.config.has_properties(), self.toolboxes.iter().map(|t| t.java_string()).collect(), "toolboxes: \n ", "\n ");
        }
        for d in &self.data {
            out.push_str(&d.java_string());
            out.push_str("\n ");
        }
        if include_sources {
            for (i, s) in self.sources.iter().enumerate() {
                out.push_str(&format!("\n Source {i}: {s}"));
            }
            out.push('\n');
        }
        out
    }

    /// `equals`: the same `toString(false)`.
    pub fn equals(&self, other: &FactoryConfiguration) -> bool {
        self.java_string(false) == other.java_string(false)
    }

    /// `createFactory()`.
    pub fn create_factory(&self) -> Result<super::toolbox::ToolboxFactory, JError> {
        let mut f = super::toolbox::ToolboxFactory::new();
        f.configure(self)?;
        Ok(f)
    }

    /// A `FactoryConfiguration` object value (what a configuration class's `getConfiguration()` returns).
    pub fn into_value(self) -> JValue {
        JValue::Object(Arc::new(ConfigurationValue(Mutex::new(self))) as Arc<dyn JObject>)
    }
}

/// A `FactoryConfiguration` as a Java object.
pub struct ConfigurationValue(pub Mutex<FactoryConfiguration>);

impl JObject for ConfigurationValue {
    fn class_name(&self) -> String {
        FACTORY_CONFIGURATION.into()
    }

    fn invoke(&self, name: &str, _args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        match name {
            "toString" => Some(Ok(Some(JValue::str(self.to_java_string())))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        self.0.lock().java_string(true)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------------------
// EasyFactoryConfiguration

/// The `EasyFactoryConfiguration` class name, the prefix of its source.
pub const EASY: &str = "org.apache.velocity.tools.config.EasyFactoryConfiguration";

/// `EasyFactoryConfiguration`: a builder over a `FactoryConfiguration` (`toolbox(scope).tool(…).property(…)`).
pub struct EasyFactoryConfiguration {
    /// The configuration built.
    pub factory: FactoryConfiguration,
    added_defaults: bool,
    /// The toolbox `toolbox(scope)` last created, by scope.
    toolbox: Option<String>,
    /// The tool `tool(…)` last added: its toolbox's scope and key.
    tool: Option<(String, Option<String>)>,
}

impl Default for EasyFactoryConfiguration {
    fn default() -> EasyFactoryConfiguration {
        EasyFactoryConfiguration::new(false)
    }
}

impl EasyFactoryConfiguration {
    /// `new EasyFactoryConfiguration(includeDefaults)`.
    pub fn new(include_defaults: bool) -> EasyFactoryConfiguration {
        EasyFactoryConfiguration::with_source(include_defaults, &include_defaults.to_string()).expect("the default tools load")
    }

    /// `new EasyFactoryConfiguration(includeDefaults, source)`: with the default tools added first when `include_defaults`; `Err` when they cannot be read.
    pub fn with_source(include_defaults: bool, source: &str) -> Result<EasyFactoryConfiguration, JError> {
        let mut e = EasyFactoryConfiguration { factory: FactoryConfiguration::for_class(EASY, source), added_defaults: false, toolbox: None, tool: None };
        if include_defaults {
            e.add_default_tools()?;
            let first = e.factory.sources.remove(0);
            e.factory.sources.push(first);
        }
        Ok(e)
    }

    /// `addDefaultTools()`: the default tools added once; `Err` when they cannot be read.
    pub fn add_default_tools(&mut self) -> Result<&mut Self, JError> {
        if !self.added_defaults {
            self.factory.add_configuration(&get_default_tools()?);
            self.added_defaults = true;
        }
        Ok(self)
    }

    /// `data(key, value)`.
    pub fn data(&mut self, key: &str, value: JValue) -> &mut Data {
        let mut d = Data::new();
        d.set_key(Some(key));
        d.set_value(value);
        self.factory.add_data(d);
        self.factory.data.iter_mut().find(|d| d.get_key() == Some(key)).expect("just added")
    }

    /// `data(key, type, value)`: `IllegalArgumentException` for an unknown type name.
    pub fn data_typed(&mut self, key: &str, t: &str, value: JValue) -> Result<&mut Self, JError> {
        self.data(key, value).set_type(Some(t))?;
        Ok(self)
    }

    fn data_type(&mut self, key: &str, t: DataType, value: JValue) -> &mut Self {
        self.data(key, value).set_type_enum(t);
        self
    }

    /// `string(key, value)`: a `string` data item.
    pub fn string(&mut self, key: &str, value: JValue) -> &mut Self {
        self.data_type(key, DataType::String, value)
    }

    /// `number(key, value)`: a `number` data item.
    pub fn number(&mut self, key: &str, value: JValue) -> &mut Self {
        self.data_type(key, DataType::Number, value)
    }

    /// `bool(key, value)`: a `boolean` data item.
    pub fn bool(&mut self, key: &str, value: JValue) -> &mut Self {
        self.data_type(key, DataType::Boolean, value)
    }

    /// `toolbox(scope)`: a new toolbox, which later `tool` and `property` calls apply to.
    pub fn toolbox(&mut self, scope: &str) -> &mut Self {
        let mut t = ToolboxConfiguration::new();
        t.set_scope(scope);
        self.factory.add_toolbox(t);
        self.toolbox = Some(scope.to_string());
        self.tool = None;
        self
    }

    /// `tool(key, classname)` on the current toolbox (a `request` one when there is none).
    pub fn tool(&mut self, key: Option<&str>, classname: &str) -> Result<&mut Self, JError> {
        if self.toolbox.is_none() {
            self.toolbox(super::toolbox::DEFAULT_SCOPE);
        }
        let scope = self.toolbox.clone().expect("a toolbox");
        let mut tool = ToolConfiguration::new();
        tool.set_classname(Some(classname));
        if let Some(k) = key {
            tool.set_key(Some(k))?;
        }
        let actual_key = tool.get_key()?;
        self.factory.get_toolbox_mut(&scope).expect("the toolbox").add_tool(tool);
        self.tool = Some((scope, actual_key));
        Ok(self)
    }

    /// `property(name, value)`: on the last tool, else the last toolbox, else the factory.
    pub fn property(&mut self, name: &str, value: JValue) -> &mut Self {
        match (&self.tool, &self.toolbox) {
            (Some((scope, key)), _) => {
                let (scope, key) = (scope.clone(), key.clone());
                if let Some(tb) = self.factory.get_toolbox_mut(&scope) {
                    if let Some(t) = tb.tools.iter_mut().find(|t| t.sort_key() == key) {
                        t.set_property(name, value);
                    }
                }
            }
            (None, Some(scope)) => {
                let scope = scope.clone();
                if let Some(tb) = self.factory.get_toolbox_mut(&scope) {
                    tb.set_property(name, value);
                }
            }
            _ => self.factory.set_property(name, value),
        }
        self
    }

    /// `restrictTo(path)`: the last tool, or every tool of the last toolbox.
    pub fn restrict_to(&mut self, path: &str) -> &mut Self {
        match (&self.tool, &self.toolbox) {
            (Some((scope, key)), _) => {
                let (scope, key) = (scope.clone(), key.clone());
                if let Some(t) = self.factory.get_toolbox_mut(&scope).and_then(|tb| tb.tools.iter_mut().find(|t| t.sort_key() == key)) {
                    t.set_restrict_to(Some(path));
                }
            }
            (None, Some(scope)) => {
                let scope = scope.clone();
                if let Some(tb) = self.factory.get_toolbox_mut(&scope) {
                    for t in tb.tools.iter_mut() {
                        t.set_restrict_to(Some(path));
                    }
                }
            }
            _ => {}
        }
        self
    }

    /// The top-level `property(name, value)`, on the factory itself.
    pub fn factory_property(&mut self, name: &str, value: JValue) -> &mut Self {
        self.factory.set_property(name, value);
        self
    }
}

// ---------------------------------------------------------------------------------------------------------
// File configurations

/// The `PropertiesFactoryConfiguration` class name, the prefix of its source.
pub const PROPERTIES_CONFIGURATION: &str = "org.apache.velocity.tools.config.PropertiesFactoryConfiguration";
/// The `XmlFactoryConfiguration` class name, the prefix of its source.
pub const XML_CONFIGURATION: &str = "org.apache.velocity.tools.config.XmlFactoryConfiguration";

/// Where `findURL` found a configuration file: its URL and content.
struct Found {
    url: String,
    text: String,
}

/// `FileFactoryConfiguration.findURL(path)` / `ConversionUtils.toURL`: a file, else a class path resource.
fn find_url(path: &str, caller: &str) -> Option<Found> {
    let p = std::path::Path::new(path);
    if p.exists() {
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        return std::fs::read(p).ok().map(|b| Found { url: format!("file:{}", abs.display()), text: String::from_utf8_lossy(&b).into_owned() });
    }
    if let Some(r) = super::classpath::get_resource(path, caller) {
        return Some(Found { url: r.url.clone(), text: r.text() });
    }
    if let Some(rest) = path.strip_prefix("file:") {
        return std::fs::read(rest).ok().map(|b| Found { url: path.to_string(), text: String::from_utf8_lossy(&b).into_owned() });
    }
    None
}

/// A `.properties` or XML `FileFactoryConfiguration`.
pub struct FileFactoryConfiguration {
    /// The configuration read.
    pub factory: FactoryConfiguration,
    xml: bool,
}

impl FileFactoryConfiguration {
    /// `new PropertiesFactoryConfiguration(id)`.
    pub fn properties(id: &str) -> FileFactoryConfiguration {
        FileFactoryConfiguration { factory: FactoryConfiguration::for_class(PROPERTIES_CONFIGURATION, id), xml: false }
    }

    /// `new XmlFactoryConfiguration(id)`.
    pub fn xml(id: &str) -> FileFactoryConfiguration {
        FileFactoryConfiguration { factory: FactoryConfiguration::for_class(XML_CONFIGURATION, id), xml: true }
    }

    fn class(&self) -> &'static str {
        if self.xml { XML_CONFIGURATION } else { PROPERTIES_CONFIGURATION }
    }

    /// `read(path)`: required.
    pub fn read(&mut self, path: &str) -> Result<(), JError> {
        self.read_required(path, true)
    }

    /// `read(path, required)`: `ResourceNotFoundException` when a required file is missing.
    pub fn read_required(&mut self, path: &str, required: bool) -> Result<(), JError> {
        match find_url(path, self.class()) {
            Some(found) => self.read_url(&found.url, &found.text),
            None if required => Err(JError::new("org.apache.velocity.exception.ResourceNotFoundException", format!("Could not find configuration file at: {path}"))),
            None => Ok(()),
        }
    }

    /// `read(url)`: the content at a URL, then the source `    .read(url)`.
    pub fn read_url(&mut self, url: &str, text: &str) -> Result<(), JError> {
        if self.xml {
            self.read_xml(text)?;
        } else {
            let mut props = crate::props::ExtProperties::new();
            props.load(text, None);
            if let Some(tools) = props.subset("tools") {
                self.read_properties_factory(&tools)?;
            }
        }
        self.factory.add_source(&format!("    .read({url})"));
        Ok(())
    }

    // ---- PropertiesFactoryConfiguration

    /// `read(ExtProperties factory)`.
    pub fn read_properties_factory(&mut self, factory: &crate::props::ExtProperties) -> Result<(), JError> {
        let mut top = std::mem::take(&mut self.factory.config);
        read_properties(factory, &mut top)?;
        self.factory.config = top;
        self.read_toolboxes(factory)?;
        if let Some(data) = factory.subset("data") {
            self.read_data(&data)?;
        }
        Ok(())
    }

    fn read_toolboxes(&mut self, factory: &crate::props::ExtProperties) -> Result<(), JError> {
        for scope in factory.string_array("toolbox") {
            let mut toolbox = ToolboxConfiguration::new();
            toolbox.set_scope(&scope);
            if let Some(props) = factory.subset(&scope) {
                read_tools(&props, &mut toolbox)?;
                read_properties(&props, &mut toolbox.config)?;
            }
            self.factory.add_toolbox(toolbox);
        }
        Ok(())
    }

    fn read_data(&mut self, dataset: &crate::props::ExtProperties) -> Result<(), JError> {
        for key in dataset.keys().to_vec() {
            if key.contains('.') {
                continue;
            }
            let mut data = Data::new();
            data.set_key(Some(&key));
            data.set_value(dataset.string(&key).map(JValue::str).unwrap_or(JValue::Null));
            if let Some(props) = dataset.subset(&key) {
                data.populate(&props)?;
            }
            self.factory.add_data(data);
        }
        Ok(())
    }

    // ---- XmlFactoryConfiguration (the Digester rules of XmlFactoryConfigurationRuleSet)

    fn read_xml(&mut self, text: &str) -> Result<(), JError> {
        let doc = super::xml::dom::parse(text).map_err(|e| {
            JError::new("java.lang.RuntimeException", format!("There was an error while parsing the InputStream: org.xml.sax.SAXParseException; lineNumber: {}; columnNumber: {}; {}", e.line, e.column, e.message))
        })?;
        let Some(root) = doc.document_element() else { return Ok(()) };
        if local(&root) != "tools" {
            return Ok(());
        }
        // `tools`: SetProperties (no writable properties) and every attribute as a property.
        for a in root.attributes() {
            let name = attribute_name(&a);
            if name != "class" {
                self.factory.set_property(&name, JValue::str(a.node_value().unwrap_or_default()));
            }
        }
        for child in root.children().into_iter().filter(|c| c.is_element()) {
            match local(&child).as_str() {
                "property" => {
                    let p = xml_property(&child)?;
                    self.factory.config.add_property(p)?;
                }
                "data" => {
                    let mut d = Data::new();
                    set_data_attributes(&mut d, &child)?;
                    if d.get_value().is_null() {
                        d.set_value(JValue::str(body(&child)));
                    }
                    self.factory.add_data(d);
                }
                "toolbox" => {
                    let mut toolbox = ToolboxConfiguration::new();
                    for a in child.attributes() {
                        if attribute_name(&a) == "scope" {
                            toolbox.set_scope(&a.node_value().unwrap_or_default());
                        }
                    }
                    for a in child.attributes() {
                        let name = attribute_name(&a);
                        if name != "class" {
                            toolbox.set_property(&name, JValue::str(a.node_value().unwrap_or_default()));
                        }
                    }
                    for t in child.children().into_iter().filter(|c| c.is_element()) {
                        match local(&t).as_str() {
                            "property" => toolbox.config.add_property(xml_property(&t)?)?,
                            "tool" => {
                                let mut tool = ToolConfiguration::new();
                                for a in t.attributes() {
                                    let v = a.node_value().unwrap_or_default();
                                    match attribute_name(&a).as_str() {
                                        "class" | "classname" => tool.set_classname(Some(&v)),
                                        "factory" | "factoryClassname" => tool.set_factory_classname(Some(&v)),
                                        _ => {}
                                    }
                                }
                                for a in t.attributes() {
                                    let v = a.node_value().unwrap_or_default();
                                    match attribute_name(&a).as_str() {
                                        "key" => tool.set_key(Some(&v))?,
                                        "restrictTo" => tool.set_restrict_to(Some(&v)),
                                        "skipSetters" => tool.set_skip_setters(Some(v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes") || v == "1" || v.eq_ignore_ascii_case("on") || v.eq_ignore_ascii_case("y"))),
                                        _ => {}
                                    }
                                }
                                for a in t.attributes() {
                                    let name = attribute_name(&a);
                                    if name != "class" {
                                        tool.set_property(&name, JValue::str(a.node_value().unwrap_or_default()));
                                    }
                                }
                                for p in t.children().into_iter().filter(|c| c.is_element() && local(c) == "property") {
                                    tool.config.add_property(xml_property(&p)?)?;
                                }
                                toolbox.add_tool(tool);
                            }
                            _ => {}
                        }
                    }
                    self.factory.add_toolbox(toolbox);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn local(n: &super::xml::Node) -> String {
    n.local_name().unwrap_or_else(|| n.node_name())
}

/// `attributes.getLocalName(i)`, else the qualified name.
fn attribute_name(a: &super::xml::Node) -> String {
    let l = a.local_name().unwrap_or_default();
    if l.is_empty() { a.node_name() } else { l }
}

/// The element's character content (`Digester`'s body text).
fn body(n: &super::xml::Node) -> String {
    n.children().iter().filter(|c| c.is_text()).map(|c| c.node_value().unwrap_or_default()).collect()
}

fn set_data_attributes(d: &mut Data, n: &super::xml::Node) -> Result<(), JError> {
    for a in n.attributes() {
        let v = a.node_value().unwrap_or_default();
        match attribute_name(&a).as_str() {
            "key" | "name" => d.set_key(Some(&v)),
            "value" => d.set_value(JValue::str(v)),
            "type" => d.set_type(Some(&v))?,
            "class" | "classname" => d.set_classname(&v)?,
            "converter" => {
                return Err(JError::new("java.lang.IllegalArgumentException", format!("Class {v} is not a valid interface org.apache.commons.beanutils.Converter")));
            }
            _ => {}
        }
    }
    Ok(())
}

/// A `<property>` element: its attributes, else its body as the value.
fn xml_property(n: &super::xml::Node) -> Result<Property, JError> {
    let mut p = Data::new();
    set_data_attributes(&mut p, n)?;
    if p.get_value().is_null() {
        p.set_value(JValue::str(body(n)));
    }
    Ok(p)
}

/// `PropertiesFactoryConfiguration.readProperties(configProps, config)`.
fn read_properties(config_props: &crate::props::ExtProperties, config: &mut Configuration) -> Result<(), JError> {
    let Some(properties) = config_props.subset("property") else { return Ok(()) };
    for name in properties.keys().to_vec() {
        let value = properties.string(&name);
        let prop_props = properties.subset(&name);
        if prop_props.as_ref().map(|p| p.keys().len()).unwrap_or(0) == 1 {
            config.set_property(&name, value.map(JValue::str).unwrap_or(JValue::Null));
        } else {
            let mut property = Data::new();
            property.set_key(Some(&name));
            property.set_value(value.map(JValue::str).unwrap_or(JValue::Null));
            if let Some(pp) = &prop_props {
                property.populate(pp)?;
            }
            // The property is built but never added, as in the original.
        }
    }
    Ok(())
}

/// `PropertiesFactoryConfiguration.readTools(tools, toolbox)`.
fn read_tools(tools: &crate::props::ExtProperties, toolbox: &mut ToolboxConfiguration) -> Result<(), JError> {
    for key in tools.keys().to_vec() {
        if key.contains('.') {
            continue;
        }
        let classname = tools.string(&key);
        let mut tool = ToolConfiguration::new();
        tool.set_classname(classname.as_deref());
        tool.set_key(Some(&key))?;
        let tool_props = tools.subset(&key);
        if let Some(tp) = &tool_props {
            read_properties(tp, &mut tool.config)?;
            let tool_key = tool.get_key()?;
            for name in tp.keys() {
                if Some(name.clone()) != tool_key {
                    tool.set_property(name, tp.string(name).map(JValue::str).unwrap_or(JValue::Null));
                }
            }
            tool.set_restrict_to(tp.string("restrictTo").as_deref());
        }
        toolbox.add_tool(tool);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// ConfigurationUtils and ConfigurationCleaner

/// The class path resource of the generic tools' default configuration.
pub const GENERIC_DEFAULTS_PATH: &str = "/org/apache/velocity/tools/generic/tools.xml";
/// The class path resource of the view tools' default configuration.
pub const VIEW_DEFAULTS_PATH: &str = "/org/apache/velocity/tools/view/tools.xml";
/// The system property (here, environment variable) naming a configuration to load.
pub const SYSTEM_PROPERTY_KEY: &str = "org.apache.velocity.tools";
/// The method `ConfigurationUtils.getFromClass` calls for a class's configuration.
pub const CONFIG_FACTORY_METHOD: &str = "getConfiguration";
const CONFIGURATION_UTILS: &str = "org.apache.velocity.tools.config.ConfigurationUtils";

/// `ConfigurationUtils.getDefaultTools()`: the generic tools' `tools.xml` (and the view tools', when present),
/// cleaned.
pub fn get_default_tools() -> Result<FactoryConfiguration, JError> {
    let mut config = FileFactoryConfiguration::xml("ConfigurationUtils.getDefaultTools()");
    config.read(GENERIC_DEFAULTS_PATH)?;
    config.read_required(VIEW_DEFAULTS_PATH, false)?;
    clean(&mut config.factory);
    Ok(config.factory)
}

/// `ConfigurationUtils.getGenericTools()`.
pub fn get_generic_tools() -> Result<FactoryConfiguration, JError> {
    let mut config = FileFactoryConfiguration::xml("ConfigurationUtils.getGenericTools()");
    config.read(GENERIC_DEFAULTS_PATH)?;
    clean(&mut config.factory);
    Ok(config.factory)
}

/// `ConfigurationUtils.findFromSystemProperty()`: the configuration the `org.apache.velocity.tools` system
/// property (here, environment variable) names.
pub fn find_from_system_property() -> Result<Option<FactoryConfiguration>, JError> {
    match std::env::var(SYSTEM_PROPERTY_KEY) {
        Ok(path) if !path.is_empty() => load(&path).map(Some),
        _ => Ok(None),
    }
}

/// `ConfigurationUtils.load(path)`: `ResourceNotFoundException` when nothing is found.
pub fn load(path: &str) -> Result<FactoryConfiguration, JError> {
    find(path)?.ok_or_else(|| JError::new("org.apache.velocity.exception.ResourceNotFoundException", format!("Could not find configuration at {path}")))
}

/// `ConfigurationUtils.find(path)`: the class path's configurations, then the file system's added.
pub fn find(path: &str) -> Result<Option<FactoryConfiguration>, JError> {
    let cp = find_in_classpath(path)?;
    let fs = find_in_file_system(path)?;
    Ok(match (cp, fs) {
        (Some(mut c), Some(f)) => {
            c.add_configuration(&f);
            Some(c)
        }
        (Some(c), None) => Some(c),
        (None, f) => f,
    })
}

/// `ConfigurationUtils.findInFileSystem(path)`: the configuration in the file at `path`, `None` when there is no such file; `UnsupportedOperationException` unless it is `.xml`, `.properties` or `.class`.
pub fn find_in_file_system(path: &str) -> Result<Option<FactoryConfiguration>, JError> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Ok(None);
    }
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let text = std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    read(&format!("file:{}", abs.display()), &text)
}

/// `ConfigurationUtils.findInClasspath(path)`.
pub fn find_in_classpath(path: &str) -> Result<Option<FactoryConfiguration>, JError> {
    let found = super::classpath::get_resources(path, CONFIGURATION_UTILS);
    match found.len() {
        0 => Ok(None),
        1 => read(&found[0].url, &found[0].text()),
        _ => {
            let mut config = FactoryConfiguration::with_source(&format!("ConfigurationUtils.findInClassPath({path},{CONFIGURATION_UTILS})"));
            let mut read_one = false;
            for r in &found {
                if let Some(c) = read(&r.url, &r.text())? {
                    read_one = true;
                    config.add_configuration(&c);
                }
            }
            Ok(read_one.then_some(config))
        }
    }
}

/// `ConfigurationUtils.read(url)`: by extension; `null` when reading fails.
pub fn read(url: &str, text: &str) -> Result<Option<FactoryConfiguration>, JError> {
    let source = format!("ConfigurationUtils.read({url})");
    let mut config = if url.ends_with(".xml") {
        FileFactoryConfiguration::xml(&source)
    } else if url.ends_with(".properties") {
        FileFactoryConfiguration::properties(&source)
    } else if url.ends_with(".class") {
        let fqn = url[..url.find('.').unwrap_or(url.len())].replace('/', ".");
        return get_from_class(&fqn).map(Some);
    } else {
        return Err(JError::new(
            "java.lang.UnsupportedOperationException",
            format!("Unknown configuration file type: {url}\nOnly .xml and .properties configuration files are supported at this time."),
        ));
    };
    match config.read_url(url, text) {
        Ok(()) => Ok(Some(config.factory)),
        Err(_) => Ok(None),
    }
}

/// `ConfigurationUtils.getFromClass(classname)`: the static (or instance) `getConfiguration()` of a class.
pub fn get_from_class(classname: &str) -> Result<FactoryConfiguration, JError> {
    let class = classes::get_class(classname).map_err(|_| JError::new("java.lang.IllegalArgumentException", format!("Could not find class {classname}")))?;
    let Some(method) = class.get_method(CONFIG_FACTORY_METHOD, &[]) else {
        return Err(JError::new("java.lang.IllegalArgumentException", format!("Could not find {CONFIG_FACTORY_METHOD} in class {}", class.name)));
    };
    let result = match &method.invoke {
        Some(f) => f(&[]).map_err(|e| JError::new("java.lang.IllegalArgumentException", format!("There was an exception while executing {CONFIG_FACTORY_METHOD} in class {}: {e}", class.name)))?,
        None => {
            let instance = class
                .new_instance()
                .map_err(|e| JError::new("java.lang.IllegalArgumentException", format!("{} must have usable default constructor or else {CONFIG_FACTORY_METHOD} must be declared static: {e}", class.name)))?;
            crate::introspect::call(&instance, CONFIG_FACTORY_METHOD, &[]).unwrap_or(Ok(None))?
        }
    };
    match result {
        Some(JValue::Object(o)) => match o.as_any().downcast_ref::<ConfigurationValue>() {
            Some(c) => Ok(c.0.lock().clone()),
            None => Err(JError::new("java.lang.ClassCastException", format!("class {} cannot be cast to class {FACTORY_CONFIGURATION}", o.class_name()))),
        },
        _ => Err(JError::new("java.lang.IllegalArgumentException", format!("Method {CONFIG_FACTORY_METHOD} in class {} should not return null or void", class.name))),
    }
}

/// `ConfigurationUtils.clean(config)` / `ConfigurationCleaner.clean(FactoryConfiguration)`: invalid data and
/// tools removed (properties are only checked: the cleaner iterates copies of them).
pub fn clean(factory: &mut FactoryConfiguration) {
    const LOG: &str = "org.apache.velocity.tools.config.ConfigurationCleaner";
    factory.config.clean_properties();
    factory.data.retain(|d| match d.validate() {
        Ok(()) => true,
        Err(e) => {
            super::log::warn(LOG, &format!("ConfigurationCleaner : Removing {} ({e})", d.java_string()));
            false
        }
    });
    for toolbox in &mut factory.toolboxes {
        clean_toolbox(toolbox);
    }
}

/// `ConfigurationCleaner.clean(ToolboxConfiguration)`.
pub fn clean_toolbox(toolbox: &mut ToolboxConfiguration) {
    toolbox.config.clean_properties();
    toolbox.tools.retain(|t| {
        t.config.clean_properties();
        match t.validate() {
            Ok(()) => true,
            Err(e) => {
                super::log::warn("org.apache.velocity.tools.config.ConfigurationCleaner", &format!("ConfigurationCleaner : Removing {} ({e})", t.java_string()));
                false
            }
        }
    });
}
