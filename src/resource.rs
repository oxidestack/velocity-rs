//! Resources (`org.apache.velocity.runtime.resource`): loaders, the string repository, and `ResourceManagerImpl`'s
//! cache with its modification checks.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};

use crate::props::{ExtProperties, PropValue};
use crate::VelocityError;

/// `System.currentTimeMillis()`: milliseconds since the Unix epoch, `0` if the clock is before it.
pub fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `StringResource`: a body, its encoding and when it was last set.
#[derive(Debug, Clone)]
pub struct StringResource {
    /// The template text.
    pub body: String,
    /// The encoding it was put in.
    pub encoding: String,
    /// When it was last put, in milliseconds; a resource put again is always later.
    pub last_modified: i64,
}

/// `StringResourceRepositoryImpl`.
#[derive(Debug)]
pub struct StringResourceRepository {
    resources: RwLock<HashMap<String, StringResource>>,
    encoding: RwLock<String>,
    /// A strictly increasing clock, so a body set twice in the same millisecond still counts as modified.
    clock: Mutex<i64>,
}

impl Default for StringResourceRepository {
    fn default() -> StringResourceRepository {
        StringResourceRepository { resources: RwLock::default(), encoding: RwLock::new(crate::config::ENCODING_DEFAULT.into()), clock: Mutex::new(0) }
    }
}

impl StringResourceRepository {
    /// `new StringResourceRepositoryImpl()`: empty, its encoding `UTF-8`.
    pub fn new() -> Arc<StringResourceRepository> {
        Arc::new(StringResourceRepository::default())
    }

    fn tick(&self) -> i64 {
        let mut c = self.clock.lock();
        *c = (*c + 1).max(now_millis());
        *c
    }

    /// `getStringResource(name)`: `None` when no resource has that name.
    pub fn get(&self, name: &str) -> Option<StringResource> {
        self.resources.read().get(name).cloned()
    }

    /// `putStringResource(name, body)`, in the repository's encoding.
    pub fn put(&self, name: &str, body: &str) {
        let encoding = self.encoding();
        self.put_with_encoding(name, body, &encoding);
    }

    /// `putStringResource(name, body, encoding)`: adds or replaces the resource, stamping it as modified now.
    pub fn put_with_encoding(&self, name: &str, body: &str, encoding: &str) {
        let last_modified = self.tick();
        self.resources.write().insert(name.to_string(), StringResource { body: body.to_string(), encoding: encoding.to_string(), last_modified });
    }

    /// `removeStringResource(name)`.
    pub fn remove(&self, name: &str) {
        self.resources.write().remove(name);
    }

    /// `getEncoding()`: the encoding new resources are put in.
    pub fn encoding(&self) -> String {
        self.encoding.read().clone()
    }

    /// `setEncoding(encoding)`.
    pub fn set_encoding(&self, encoding: &str) {
        *self.encoding.write() = encoding.to_string();
    }
}

static STATIC_REPOSITORIES: LazyLock<Mutex<HashMap<String, Arc<StringResourceRepository>>>> = LazyLock::new(Default::default);

/// `StringResourceLoader.getRepository(name)` (the static repositories).
pub fn static_repository(name: &str) -> Option<Arc<StringResourceRepository>> {
    STATIC_REPOSITORIES.lock().get(name).cloned()
}

/// `StringResourceLoader.setRepository(name, repo)`: registers a static repository under `name`.
pub fn set_static_repository(name: &str, repo: Arc<StringResourceRepository>) {
    STATIC_REPOSITORIES.lock().insert(name.to_string(), repo);
}

/// `StringResourceLoader.removeRepository(name)`: the repository that was registered, if any.
pub fn remove_static_repository(name: &str) -> Option<Arc<StringResourceRepository>> {
    STATIC_REPOSITORIES.lock().remove(name)
}

/// `StringResourceLoader.REPOSITORY_NAME_DEFAULT`: the repository name used when `repository.name` is not set.
pub const DEFAULT_REPOSITORY_NAME: &str = "org.apache.velocity.runtime.resource.util.StringResourceRepository";

/// What a loader needs from the engine while initialising.
pub struct LoaderInit<'a> {
    /// The loader's own properties (`resource.loader.<id>.*`, without the prefix).
    pub config: &'a ExtProperties,
    /// The engine's application attributes, where a non-static string repository is kept.
    pub attributes: &'a Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    /// `resource.default_encoding`.
    pub default_encoding: &'a str,
}

/// `org.apache.velocity.runtime.resource.loader.ResourceLoader`.
pub trait ResourceLoader: Send + Sync {
    /// `getClassName()`: the configured class.
    fn class_name(&self) -> String;

    /// `getResourceReader` read to the end: the template text, or a `ResourceNotFoundException`.
    fn read(&self, name: &str, encoding: Option<&str>) -> Result<String, VelocityError>;

    /// `resourceExists(name)`: whether this loader can read the resource.
    fn exists(&self, name: &str) -> bool {
        self.read(name, None).is_ok()
    }

    /// `isSourceModified`, given what the cached resource recorded.
    fn is_source_modified(&self, name: &str, last_modified: i64) -> bool;

    /// `getLastModified(resource)`: when the source last changed, in milliseconds; `0` when unknown.
    fn last_modified(&self, name: &str) -> i64;

    /// `resource.loader.<id>.cache` and `.modification_check_interval`.
    fn common(&self) -> &LoaderSettings;
}

/// `ResourceLoader.commonInit`.
#[derive(Debug, Clone, Default)]
pub struct LoaderSettings {
    /// `isCachingOn()`: `resource.loader.<id>.cache`.
    pub caching_on: bool,
    /// `getModificationCheckInterval()`: `resource.loader.<id>.modification_check_interval`, in seconds; `0` or less
    /// never checks.
    pub check_interval_secs: i64,
}

impl LoaderSettings {
    /// `commonInit`: reads `cache` and `modification_check_interval`; `Err` when either does not parse.
    pub fn from(config: &ExtProperties) -> Result<LoaderSettings, VelocityError> {
        let caching_on = config
            .get_boolean("cache", false)
            .map_err(|_| VelocityError::velocity(format!("Exception parsing cache setting: {}", config.string("cache").unwrap_or_default())))?;
        let check_interval_secs = config
            .get_long("modification_check_interval", 0)
            .map_err(|_| VelocityError::velocity("Exception parsing modificationCheckInterval setting: modification_check_interval"))?;
        Ok(LoaderSettings { caching_on, check_interval_secs })
    }
}

/// `FileResourceLoader`: the first readable `path/name`; a leading `/` in the name is dropped (except with an empty
/// path).
pub struct FileResourceLoader {
    paths: Vec<String>,
    found_in: Mutex<HashMap<String, String>>,
    settings: LoaderSettings,
    class: String,
}

impl FileResourceLoader {
    /// `FileResourceLoader.init`: the search path from `path`; `Err` when the common settings do not parse.
    pub fn new(init: &LoaderInit, class: &str) -> Result<FileResourceLoader, VelocityError> {
        Ok(FileResourceLoader {
            paths: init.config.string_array("path").into_iter().map(|p| p.trim().to_string()).collect(),
            found_in: Mutex::default(),
            settings: LoaderSettings::from(init.config)?,
            class: class.to_string(),
        })
    }

    fn file(path: &str, name: &str) -> PathBuf {
        if path.is_empty() {
            PathBuf::from(name)
        } else {
            Path::new(path).join(name.strip_prefix('/').unwrap_or(name))
        }
    }

    fn readable(p: &Path) -> bool {
        p.is_file() && std::fs::File::open(p).is_ok()
    }

    fn modified(p: &Path) -> i64 {
        std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
    }
}

/// Decodes bytes in `encoding` (UTF-8, ISO-8859-1 / US-ASCII); a byte-order mark wins over the asked encoding.
pub fn decode(bytes: &[u8], encoding: Option<&str>) -> String {
    use std::io::Read;
    // `ResourceLoader.buildReader`: a byte order mark decides the encoding.
    let mut stream = crate::io::UnicodeInputStream::new(bytes, true).expect("reading a byte slice");
    let found = stream.encoding_from_stream();
    let mut rest = Vec::with_capacity(bytes.len());
    stream.read_to_end(&mut rest).expect("reading a byte slice");
    let encoding = found.or(encoding);
    let utf16 = |be: bool| String::from_utf16_lossy(&rest.chunks(2).map(|c| if be { u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]) } else { u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]) }).collect::<Vec<_>>());
    let utf32 = |be: bool| {
        rest.chunks(4)
            .map(|c| {
                let mut b = [0u8; 4];
                b[..c.len()].copy_from_slice(c);
                char::from_u32(if be { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }).unwrap_or('\u{fffd}')
            })
            .collect::<String>()
    };
    match encoding.map(|e| e.to_ascii_uppercase()).as_deref() {
        Some("ISO-8859-1" | "ISO8859_1" | "LATIN1" | "US-ASCII" | "ASCII") => rest.iter().map(|b| *b as char).collect(),
        Some("UTF-16" | "UTF-16BE") => utf16(true),
        Some("UTF-16LE") => utf16(false),
        Some("UTF-32" | "UTF-32BE") => utf32(true),
        Some("UTF-32LE") => utf32(false),
        Some(label) => match encoding_rs::Encoding::for_label(label.as_bytes()) {
            Some(e) => e.decode_without_bom_handling(&rest).0.into_owned(),
            None => String::from_utf8_lossy(&rest).into_owned(),
        },
        None => String::from_utf8_lossy(&rest).into_owned(),
    }
}

impl ResourceLoader for FileResourceLoader {
    fn class_name(&self) -> String {
        self.class.clone()
    }

    fn read(&self, name: &str, encoding: Option<&str>) -> Result<String, VelocityError> {
        if name.is_empty() {
            return Err(VelocityError::resource_not_found("Need to specify a file name or file path!"));
        }
        for path in &self.paths {
            let file = FileResourceLoader::file(path, name);
            if FileResourceLoader::readable(&file) {
                let bytes = std::fs::read(&file).map_err(|_| VelocityError::velocity(format!("Exception while loading Template {name}")))?;
                self.found_in.lock().insert(name.to_string(), path.clone());
                return Ok(decode(&bytes, encoding));
            }
        }
        Err(VelocityError::resource_not_found(format!("FileResourceLoader: cannot find {name}")))
    }

    fn exists(&self, name: &str) -> bool {
        !name.is_empty() && self.paths.iter().any(|p| FileResourceLoader::readable(&FileResourceLoader::file(p, name)))
    }

    fn is_source_modified(&self, name: &str, last_modified: i64) -> bool {
        let path = self.found_in.lock().get(name).cloned().unwrap_or_default();
        let current = self.paths.iter().map(|p| FileResourceLoader::file(p, name)).find(|f| FileResourceLoader::readable(f));
        let file = FileResourceLoader::file(&path, name);
        match current {
            Some(c) if file.exists() && c == file && FileResourceLoader::readable(&file) => FileResourceLoader::modified(&file) != last_modified,
            _ => true,
        }
    }

    fn last_modified(&self, name: &str) -> i64 {
        let path = self.found_in.lock().get(name).cloned().unwrap_or_default();
        let file = FileResourceLoader::file(&path, name);
        if FileResourceLoader::readable(&file) { FileResourceLoader::modified(&file) } else { 0 }
    }

    fn common(&self) -> &LoaderSettings {
        &self.settings
    }
}

/// `StringResourceLoader`: templates from a repository, static (shared by name across engines) or kept in the
/// engine's application attributes.
pub struct StringResourceLoader {
    /// The repository templates are read from.
    pub repository: Arc<StringResourceRepository>,
    settings: LoaderSettings,
    class: String,
}

impl StringResourceLoader {
    /// `StringResourceLoader.init`: finds or creates the repository named by `repository.name` (static unless
    /// `repository.static` is `false`), setting its encoding from `repository.encoding`.
    pub fn new(init: &LoaderInit, class: &str) -> Result<StringResourceLoader, VelocityError> {
        let name = init.config.string_or("repository.name", DEFAULT_REPOSITORY_NAME);
        let is_static = init.config.boolean("repository.static", true);
        let encoding = init.config.string("repository.encoding");
        let existing = if is_static {
            static_repository(&name)
        } else {
            init.attributes.lock().get(&name).and_then(|a| a.clone().downcast::<StringResourceRepository>().ok())
        };
        let repository = match existing {
            Some(r) => {
                if let Some(e) = &encoding {
                    r.set_encoding(e);
                }
                r
            }
            None => {
                let r = StringResourceRepository::new();
                r.set_encoding(encoding.as_deref().unwrap_or(crate::config::ENCODING_DEFAULT));
                if is_static {
                    set_static_repository(&name, r.clone());
                } else {
                    init.attributes.lock().insert(name.clone(), r.clone());
                }
                r
            }
        };
        Ok(StringResourceLoader { repository, settings: LoaderSettings::from(init.config)?, class: class.to_string() })
    }
}

impl ResourceLoader for StringResourceLoader {
    fn class_name(&self) -> String {
        self.class.clone()
    }

    fn read(&self, name: &str, _encoding: Option<&str>) -> Result<String, VelocityError> {
        if name.is_empty() {
            return Err(VelocityError::resource_not_found("No template name provided"));
        }
        match self.repository.get(name) {
            Some(r) => Ok(r.body),
            None => Err(VelocityError::resource_not_found(format!("Could not locate resource '{name}'"))),
        }
    }

    fn exists(&self, name: &str) -> bool {
        self.repository.get(name).is_some()
    }

    fn is_source_modified(&self, name: &str, last_modified: i64) -> bool {
        match self.repository.get(name) {
            Some(r) => r.last_modified != last_modified,
            None => true,
        }
    }

    fn last_modified(&self, name: &str) -> i64 {
        self.repository.get(name).map(|r| r.last_modified).unwrap_or(0)
    }

    fn common(&self) -> &LoaderSettings {
        &self.settings
    }
}

/// Builds a loader from its `resource.loader.<id>.*` configuration.
pub type LoaderFactory = Arc<dyn Fn(&LoaderInit) -> Result<Arc<dyn ResourceLoader>, VelocityError> + Send + Sync>;

/// `FileResourceLoader`'s class name.
pub const FILE_LOADER: &str = "org.apache.velocity.runtime.resource.loader.FileResourceLoader";
/// `StringResourceLoader`'s class name.
pub const STRING_LOADER: &str = "org.apache.velocity.runtime.resource.loader.StringResourceLoader";

static LOADER_CLASSES: LazyLock<RwLock<HashMap<String, LoaderFactory>>> = LazyLock::new(|| {
    let mut m: HashMap<String, LoaderFactory> = HashMap::new();
    m.insert(FILE_LOADER.into(), Arc::new(|i: &LoaderInit| Ok(Arc::new(FileResourceLoader::new(i, FILE_LOADER)?) as Arc<dyn ResourceLoader>)));
    m.insert(STRING_LOADER.into(), Arc::new(|i: &LoaderInit| Ok(Arc::new(StringResourceLoader::new(i, STRING_LOADER)?) as Arc<dyn ResourceLoader>)));
    RwLock::new(m)
});

/// Makes a loader class name known (`ClassUtils.getNewInstance` for loaders).
pub fn register_loader_class(class: &str, factory: LoaderFactory) {
    LOADER_CLASSES.write().insert(class.to_string(), factory);
}

/// A loader instance set programmatically (`resource.loader.<id>.instance`), ready to use.
#[derive(Clone)]
pub struct LoaderInstance(pub Arc<dyn ResourceLoader>);

/// A loader instance the engine still initializes with `resource.loader.<id>.*` (`commonInit` and `init`), as
/// Velocity does with a `new FileResourceLoader()` passed as `.instance`.
#[derive(Clone)]
pub struct UninitializedLoader(pub LoaderFactory);

/// The constructor registered for a loader class (`new FileResourceLoader()`, …).
pub fn loader_class(class: &str) -> Option<LoaderFactory> {
    LOADER_CLASSES.read().get(class).cloned()
}

/// A loaded, parsed template (or content) and what the cache needs to refresh it.
pub struct CachedResource<T> {
    /// The resource's name.
    pub name: String,
    /// The processed resource: the parsed template, or the content.
    pub data: Arc<T>,
    /// The encoding it was read in.
    pub encoding: String,
    /// The index of the loader it came from, in [`ResourceManager::loaders`].
    pub loader: usize,
    /// The source's modification time when loaded (`Resource.getLastModified()`).
    pub last_modified: i64,
    /// The loader's modification check interval, in seconds; `0` or less never checks.
    pub check_interval_secs: i64,
    /// When the source is next checked for changes, in milliseconds.
    pub next_check: i64,
}

impl<T> CachedResource<T> {
    fn requires_checking(&self) -> bool {
        self.check_interval_secs > 0 && now_millis() >= self.next_check
    }

    fn touch(&mut self) {
        self.next_check = now_millis() + 1000 * self.check_interval_secs;
    }
}

/// The resource cache: the entries by name, and their names oldest first for the LRU eviction.
type Cache<T> = (HashMap<String, CachedResource<T>>, VecDeque<String>);

/// `ResourceManagerImpl` over one kind of processed resource (a parsed template, or raw content).
pub struct ResourceManager<T> {
    /// The resource loaders, in `resource.loaders` order.
    pub loaders: Vec<Arc<dyn ResourceLoader>>,
    cache: Mutex<Cache<T>>,
    cache_size: usize,
}

impl<T> ResourceManager<T> {
    /// `initialize`: a loader per `resource.loaders` entry that has configuration.
    pub fn new(config: &ExtProperties, attributes: &Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>, default_encoding: &str) -> Result<ResourceManager<T>, VelocityError> {
        let mut loaders: Vec<Arc<dyn ResourceLoader>> = Vec::new();
        for id in config.string_array(crate::config::RESOURCE_LOADERS) {
            let id = id.trim().to_string();
            let Some(mut sub) = config.subset(&format!("resource.loader.{id}")) else { continue };
            sub.set_property("_RESOURCE_LOADER_IDENTIFIER_", PropValue::Str(id.clone()));
            let init = LoaderInit { config: &sub, attributes, default_encoding };
            if let Some(instance) = sub.object::<LoaderInstance>("instance") {
                loaders.push(instance.0.clone());
                continue;
            }
            if let Some(instance) = sub.object::<UninitializedLoader>("instance") {
                loaders.push((instance.0)(&init)?);
                continue;
            }
            let Some(class) = sub.string("class") else {
                return Err(VelocityError::velocity(format!("Unable to find 'resource.loader.{id}.class' specification in configuration. This is a critical value.  Please adjust configuration.")));
            };
            let factory = LOADER_CLASSES.read().get(&class).cloned();
            match factory {
                Some(f) => loaders.push(f(&init)?),
                None => {
                    return Err(VelocityError::velocity(format!(
                        "The specified class for ResourceLoader ({class}) does not exist or is not accessible to the current classloader."
                    )))
                }
            }
        }
        let size = config.int(crate::config::RESOURCE_MANAGER_DEFAULTCACHE_SIZE, 89);
        Ok(ResourceManager { loaders, cache: Mutex::default(), cache_size: if size > 0 { size as usize } else { usize::MAX } })
    }

    fn cache_put(&self, key: String, r: CachedResource<T>) {
        let mut cache = self.cache.lock();
        if cache.0.insert(key.clone(), r).is_some() {
            cache.1.retain(|k| *k != key);
        }
        cache.1.push_back(key);
        while cache.0.len() > self.cache_size {
            match cache.1.pop_front() {
                Some(old) => {
                    cache.0.remove(&old);
                }
                None => break,
            }
        }
    }

    fn cache_touch(&self, key: &str) {
        let mut cache = self.cache.lock();
        if let Some(i) = cache.1.iter().position(|k| k == key) {
            let k = cache.1.remove(i).expect("present");
            cache.1.push_back(k);
        }
    }

    /// Empties the resource cache, so every resource is loaded again.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock();
        cache.0.clear();
        cache.1.clear();
    }

    /// `loadResource`: the first loader that has it, then `process` (parsing, for templates).
    fn load(&self, name: &str, encoding: &str, process: &dyn Fn(&str, &str) -> Result<T, VelocityError>) -> Result<CachedResource<T>, VelocityError> {
        for (i, loader) in self.loaders.iter().enumerate() {
            match loader.read(name, Some(encoding)) {
                Ok(text) => {
                    let data = Arc::new(process(name, &text)?);
                    let mut r = CachedResource {
                        name: name.to_string(),
                        data,
                        encoding: encoding.to_string(),
                        loader: i,
                        last_modified: loader.last_modified(name),
                        check_interval_secs: loader.common().check_interval_secs,
                        next_check: 0,
                    };
                    r.touch();
                    return Ok(r);
                }
                Err(e) if e.kind == crate::ErrorKind::ResourceNotFound => continue,
                Err(e) => return Err(e),
            }
        }
        Err(VelocityError::resource_not_found(format!("Unable to find resource '{name}'")))
    }

    /// `getResource(name, type, encoding)`.
    pub fn get(&self, kind: u8, name: &str, encoding: &str, process: &dyn Fn(&str, &str) -> Result<T, VelocityError>) -> Result<Arc<T>, VelocityError> {
        let key = format!("{kind}{name}");
        let cached = {
            let cache = self.cache.lock();
            cache.0.get(&key).map(|r| (r.requires_checking(), r.loader, r.last_modified, r.data.clone()))
        };
        if let Some((requires_checking, loader, last_modified, data)) = cached {
            self.cache_touch(&key);
            if !requires_checking {
                return Ok(data);
            }
            return match self.refresh(&key, name, encoding, loader, last_modified, process) {
                Ok(d) => Ok(d),
                Err(e) if e.kind == crate::ErrorKind::ResourceNotFound => {
                    self.cache.lock().0.remove(&key);
                    self.get(kind, name, encoding, process)
                }
                Err(e) => Err(e),
            };
        }
        let r = self.load(name, encoding, process)?;
        let data = r.data.clone();
        if self.loaders[r.loader].common().caching_on {
            self.cache_put(key, r);
        }
        Ok(data)
    }

    /// `refreshResource`.
    fn refresh(&self, key: &str, name: &str, encoding: &str, loader: usize, last_modified: i64, process: &dyn Fn(&str, &str) -> Result<T, VelocityError>) -> Result<Arc<T>, VelocityError> {
        if let Some(r) = self.cache.lock().0.get_mut(key) {
            r.touch();
        }
        // A resource found by a later loader is reloaded when an earlier loader now has it.
        if loader > 0 && self.loader_for(name) != Some(loader) {
            let r = self.load(name, encoding, process)?;
            let data = r.data.clone();
            if self.loaders[r.loader].common().caching_on {
                self.cache_put(key.to_string(), r);
            }
            return Ok(data);
        }
        let l = &self.loaders[loader];
        if l.is_source_modified(name, last_modified) {
            let text = l.read(name, Some(encoding))?;
            let data = Arc::new(process(name, &text)?);
            let mut r = CachedResource {
                name: name.to_string(),
                data: data.clone(),
                encoding: encoding.to_string(),
                loader,
                last_modified: l.last_modified(name),
                check_interval_secs: l.common().check_interval_secs,
                next_check: 0,
            };
            r.touch();
            self.cache_put(key.to_string(), r);
            return Ok(data);
        }
        Ok(self.cache.lock().0.get(key).map(|r| r.data.clone()).expect("cached"))
    }

    /// The loader that has `name` and its modification time there (`Twonk`'s template and time).
    pub fn stamp(&self, name: &str) -> Option<(usize, i64)> {
        self.loader_for(name).map(|i| (i, self.loaders[i].last_modified(name)))
    }

    /// `template.getResourceLoader().getLastModified(template)`.
    pub fn last_modified_in(&self, loader: usize, name: &str) -> i64 {
        self.loaders.get(loader).map(|l| l.last_modified(name)).unwrap_or(0)
    }

    fn loader_for(&self, name: &str) -> Option<usize> {
        self.loaders.iter().position(|l| l.exists(name))
    }

    /// `getLoaderNameForResource`: `class <name>` of the first loader that has it.
    pub fn loader_name_for(&self, name: &str) -> Option<String> {
        self.loader_for(name).map(|i| format!("class {}", self.loaders[i].class_name()))
    }
}

/// `Resource.getType()`: what a resource manager is asked to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// `RuntimeConstants.RESOURCE_TEMPLATE`: a template, parsed.
    Template,
    /// `RuntimeConstants.RESOURCE_CONTENT`: raw content, as `#include` reads it.
    Content,
}

/// `org.apache.velocity.runtime.resource.ResourceManager`: a replacement for the engine's own resource management
/// (`resource.manager.instance`, an `Arc<dyn ResourceManagerApi>`).
pub trait ResourceManagerApi: Send + Sync {
    /// `initialize(rsvc)`.
    fn initialize(&self, _rsvc: &crate::Engine) -> Result<(), VelocityError> {
        Ok(())
    }

    /// `getResource(name, type, encoding)`: `None` for Java's `null`.
    fn get_resource(&self, name: &str, kind: ResourceType, encoding: &str) -> Result<Option<Arc<crate::Loaded>>, VelocityError>;

    /// `getLoaderNameForResource(name)`.
    fn loader_name_for_resource(&self, name: &str) -> Option<String>;
}
