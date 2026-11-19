//! Velocity Engine 2.4.1's own test suite (`velocity-engine-core/src/test/java/org/apache/velocity/test`), ported
//! class by class; each module names the Java class it ports.

mod base;

mod arithmetic;
mod alternate_values;
mod if_null;
mod string_concatenation;
mod if_empty;
mod index;
mod io_cases;
mod issues1;
mod issues2;
mod introspector;
mod macro_libraries;
mod macros;
mod method_calls;
mod misc_cases;
mod break_directive;
mod builtin_event_handlers;
mod comments;
mod conversion;
mod define;
mod evaluate;
mod event_handling;
mod exceptions;
mod foreach;
mod get_as;
mod set;
mod stop_directive;
mod block_macro;
mod provider;
mod resources;
mod scope;
mod space_gobbling;
mod strict;
mod templates;
mod test_logger;
mod textblock;
mod uberspectors;
