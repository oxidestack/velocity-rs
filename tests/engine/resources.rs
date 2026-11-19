//! `ResourceCachingTestCase`, `ResourceExistsTestCase`, `ResourceLoaderInstanceTestCase`,
//! `StringResourceLoaderRepositoryTestCase`, `StringResourceLoaderTestCase`, `MultipleFileResourcePathTestCase`,
//! `AbsoluteFileResourceLoaderTestCase`, `EncodingTestCase`.

use std::sync::Arc;

use java_compat::lang::JValue;
use velocity::props::{ExtProperties, PropValue};
use velocity::resource::{self, StringResourceRepository, UninitializedLoader};
use velocity::{Context, Engine};

use crate::base::*;
use crate::test_logger::TestLogger;

fn props(entries: &[(&str, &str)]) -> ExtProperties {
    let mut p = ExtProperties::new();
    let instance: Arc<dyn velocity::log::Logger> = Arc::new(TestLogger::default());
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    for (k, v) in entries {
        p.add_property(k, (*v).into());
    }
    p
}

fn merge(engine: &Engine, name: &str, context: &mut Context) -> String {
    let t = engine.get_template(name, None).unwrap_or_else(|e| panic!("{name}: {}", e.message()));
    engine.merge(&t, context).unwrap_or_else(|e| panic!("{name}: {}", e.message()))
}

#[track_caller]
fn assert_matches(out: &str, compare: &str) {
    assert_eq!(normalize_newlines(out), normalize_newlines(&file_contents(&resource(compare))), "Output incorrect for {compare}");
}

#[test]
fn test_include_parse_caching() {
    let engine = Engine::with_properties(&props(&[("file.resource.loader.cache", "true"), ("file.resource.loader.path", &resource("resourcecaching"))])).unwrap();
    merge(&engine, "testincludeparse.vm", &mut Context::new());
}

fn exists_engine() -> Engine {
    let logger = Arc::new(TestLogger::default());
    logger.on();
    let mut p = ExtProperties::new();
    p.set_property("resource.loader", "file,string".into());
    p.set_property("file.resource.loader.path", resource("resourceexists").into());
    p.set_property("string.resource.loader.class", resource::STRING_LOADER.into());
    let instance: Arc<dyn velocity::log::Logger> = logger;
    p.set_property(velocity::log::RUNTIME_LOG_INSTANCE, PropValue::Object(Arc::new(instance)));
    p.set_property("runtime.log.logsystem.test.level", "debug".into());
    Engine::with_properties(&p).unwrap()
}

#[test]
fn test_file_resource_exists() {
    let velocity = exists_engine();
    assert!(velocity.resource_exists("testfile.vm"), "testfile.vm was not found");
    assert!(!velocity.resource_exists("nosuchfile.vm"), "nosuchfile.vm should not have been found");
}

#[test]
fn test_string_resource_exists() {
    let velocity = exists_engine();
    assert!(!velocity.resource_exists("foo.vm"));
    default_repository().put("foo.vm", "Make it so!");
    assert!(velocity.resource_exists("foo.vm"));
}

/// `StringResourceLoader.getRepository()`.
fn default_repository() -> Arc<StringResourceRepository> {
    match resource::static_repository(resource::DEFAULT_REPOSITORY_NAME) {
        Some(r) => r,
        None => {
            let r = StringResourceRepository::new();
            resource::set_static_repository(resource::DEFAULT_REPOSITORY_NAME, r.clone());
            r
        }
    }
}

#[test]
fn test_resource_loader_instance() {
    let file_loader = resource::loader_class(resource::FILE_LOADER).expect("the file loader");
    let mut p = props(&[]);
    p.set_property("resource.loader", "testrl".into());
    p.set_property("testrl.resource.loader.instance", PropValue::Object(Arc::new(UninitializedLoader(file_loader))));
    p.set_property("testrl.resource.loader.path", resource("resourceinstance").into());
    let engine = Engine::with_properties(&p).unwrap();
    assert_matches(&merge(&engine, "testfile.vm", &mut Context::new()), "resourceinstance/compare/testfile.cmp");
}

/// `Velocity.init()` with a string loader over the default static repository.
fn string_singleton() -> Engine {
    Engine::with_properties(&props(&[
        ("resource.loaders", "string"),
        ("string.resource.loader.class", resource::STRING_LOADER),
        ("string.resource.loader.modificationCheckInterval", "1"),
    ]))
    .unwrap()
}

fn repository_context() -> Context {
    let repo = default_repository();
    repo.put("foo", "This is $foo");
    repo.put("bar", "This is $bar");
    let mut context = Context::new();
    context.put("foo", s("wonderful!"));
    context.put("bar", s("horrible!"));
    context.put("woogie", s("a woogie"));
    context
}

fn new_string_engine(repo_name: Option<&str>, is_static: bool) -> ExtProperties {
    let mut p = props(&[("resource.loaders", "string"), ("string.resource.loader.class", resource::STRING_LOADER)]);
    if let Some(name) = repo_name {
        p.add_property("string.resource.loader.repository.name", name.into());
    }
    if !is_static {
        p.add_property("string.resource.loader.repository.static", "false".into());
    }
    p.add_property("string.resource.loader.modificationCheckInterval", "1".into());
    p
}

#[test]
fn test_shared_repo() {
    let singleton = string_singleton();
    let mut context = repository_context();
    let engine = Engine::with_properties(&new_string_engine(None, true)).unwrap();
    assert_eq!(merge(&engine, "foo", &mut context), merge(&singleton, "foo", &mut context));
}

#[test]
fn test_alternate_static_repo() {
    let singleton = string_singleton();
    let mut context = repository_context();
    let p = new_string_engine(Some("alternate.repo"), true);
    assert!(resource::static_repository("alternate.repo").is_none());
    let engine = Engine::with_properties(&p).unwrap();
    let repo = resource::static_repository("alternate.repo").expect("created by init");
    repo.put("foo", "This is NOT $foo");
    assert_ne!(merge(&engine, "foo", &mut context), merge(&singleton, "foo", &mut context));
}

#[test]
fn test_pre_created_static_repo() {
    let mut context = repository_context();
    let p = new_string_engine(Some("my.repo"), true);
    let repo = StringResourceRepository::new();
    repo.put("bar", "This is NOT $bar");
    resource::set_static_repository("my.repo", repo);
    let engine = Engine::with_properties(&p).unwrap();
    assert_eq!(merge(&engine, "bar", &mut context), "This is NOT horrible!");
}

#[test]
fn test_app_repo() {
    let mut context = repository_context();
    let engine = Engine::with_properties(&new_string_engine(None, false)).unwrap();
    let repo = engine.string_repository(resource::DEFAULT_REPOSITORY_NAME).expect("an application repository");
    repo.put("woogie", "What is $woogie?");
    assert_eq!(merge(&engine, "woogie", &mut context), "What is a woogie?");
}

#[test]
fn test_alternate_app_repo() {
    let mut context = repository_context();
    let engine = Engine::with_properties(&new_string_engine(Some("alternate.app.repo"), false)).unwrap();
    let repo = engine.string_repository("alternate.app.repo").expect("an application repository");
    repo.put("you/foo.vm", "You look $foo");
    assert_eq!(merge(&engine, "you/foo.vm", &mut context), "You look wonderful!");
}

#[test]
fn test_pre_created_app_repo() {
    let mut context = repository_context();
    let repo = StringResourceRepository::new();
    repo.put("you/bar.vm", "You look $bar");
    // Set before the engine initialises on first use.
    let engine = Engine::with_properties_and_attributes(&new_string_engine(Some("my.app.repo"), false), vec![("my.app.repo".into(), repo)]).unwrap();
    assert_eq!(merge(&engine, "you/bar.vm", &mut context), "You look horrible!");
}

#[test]
fn test_simple_template() {
    let engine = string_singleton();
    default_repository().put("simpletemplate.vm", "This is a test for ${foo}");
    let mut context = Context::new();
    context.put("foo", s("a foo object"));
    assert_matches(&merge(&engine, "simpletemplate.vm", &mut context), "stringloader/compare/simpletemplate.cmp");
}

#[test]
fn test_multiple_templates() {
    let engine = string_singleton();
    default_repository().put("multi1.vm", "I am the $first template.");
    default_repository().put("multi2.vm", "I am the $second template.");
    let mut context = Context::new();
    context.put("first", JValue::Int(1));
    context.put("second", s("two"));
    assert_matches(&merge(&engine, "multi1.vm", &mut context), "stringloader/compare/multi1.cmp");
    assert_matches(&merge(&engine, "multi2.vm", &mut context), "stringloader/compare/multi2.cmp");
}

#[test]
fn test_content_change() {
    let engine = string_singleton();
    default_repository().put("change.vm", "I am the $first template.");
    let mut context = Context::new();
    context.put("first", JValue::Int(1));
    context.put("second", s("two"));
    let out1 = merge(&engine, "change.vm", &mut context);
    default_repository().put("change.vm", "I am the $second template.");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let out2 = merge(&engine, "change.vm", &mut context);
    assert_matches(&out1, "stringloader/compare/change1.cmp");
    assert_matches(&out2, "stringloader/compare/change2.cmp");
}

#[test]
fn test_multiple_file_resources() {
    let engine = Engine::with_properties(&props(&[("file.resource.loader.path", &resource("multi/path1")), ("file.resource.loader.path", &resource("multi/path2"))])).unwrap();
    let mut context = Context::new();
    assert_matches(&merge(&engine, "path1.vm", &mut context), "multi/compare/path1.cmp");
    assert_matches(&merge(&engine, "path2.vm", &mut context), "multi/compare/path2.cmp");
}

#[test]
fn test_absolute_file_resource_loader() {
    // An empty path: names are absolute (or relative to the working directory).
    let engine = Engine::with_properties(&props(&[("file.resource.loader.path", "")])).unwrap();
    let f = format!("{}.vm", resource("absolute/absolute"));
    assert_matches(&merge(&engine, &f, &mut Context::new()), "absolute/compare/absolute.cmp");
}

fn encoding_engine() -> Engine {
    Engine::with_properties(&props(&[("file.resource.loader.path", &resource("templates")), ("input.encoding", "UTF-8")])).unwrap()
}

/// Merges `name` read as `encoding`, writes it in `encoding` and compares the bytes with the `.cmp`.
fn encoding_test(name: &str, encoding: &str) {
    let engine = encoding_engine();
    let t = engine.get_template(&format!("{name}.vm"), Some(encoding)).unwrap();
    let out = engine.merge(&t, &mut Context::new()).unwrap();
    let (bytes, _, _) = encoding_rs::Encoding::for_label(encoding.as_bytes()).expect("a charset").encode(&out);
    let compare = std::fs::read(resource(&format!("templates/compare/{name}.cmp"))).unwrap();
    assert_eq!(String::from_utf8_lossy(&bytes), String::from_utf8_lossy(&compare), "Output incorrect for {name}");
}

#[test]
fn test_chinese_encoding() {
    encoding_test("encodingtest", "UTF-8");
}

#[test]
fn test_high_byte_chinese() {
    encoding_test("encodingtest2", "UTF-8");
}

#[test]
fn test_high_byte_chinese2() {
    encoding_test("encodingtest3", "GBK");
}

#[test]
fn test_russian() {
    encoding_test("encodingtest_KOI8-R", "KOI8-R");
}
